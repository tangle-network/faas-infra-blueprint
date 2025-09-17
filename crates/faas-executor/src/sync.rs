use chrono::{DateTime, Utc};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Advanced file synchronization with gitignore support
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncOptions {
    pub use_gitignore: bool,
    pub dry_run: bool,
    pub delete_unmatched: bool,
    pub checksum_only: bool,
    pub preserve_timestamps: bool,
    pub exclude_patterns: Vec<String>,
    pub include_patterns: Vec<String>,
}

impl Default for SyncOptions {
    fn default() -> Self {
        Self {
            use_gitignore: true,
            dry_run: false,
            delete_unmatched: false,
            checksum_only: false,
            preserve_timestamps: true,
            exclude_patterns: vec![],
            include_patterns: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResult {
    pub files_copied: Vec<String>,
    pub files_updated: Vec<String>,
    pub files_deleted: Vec<String>,
    pub files_skipped: Vec<String>,
    pub bytes_transferred: u64,
    pub duration_ms: u64,
    pub dry_run: bool,
}

#[derive(Debug, Clone)]
struct FileInfo {
    path: PathBuf,
    size: u64,
    modified: DateTime<Utc>,
    checksum: Option<String>,
}

pub struct FileSynchronizer {
    gitignore: Option<Gitignore>,
    options: SyncOptions,
}

impl FileSynchronizer {
    pub async fn new(
        base_path: &Path,
        options: SyncOptions,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let gitignore = if options.use_gitignore {
            Self::load_gitignore(base_path).await?
        } else {
            None
        };

        Ok(Self { gitignore, options })
    }

    async fn load_gitignore(
        base_path: &Path,
    ) -> Result<Option<Gitignore>, Box<dyn std::error::Error>> {
        let gitignore_path = base_path.join(".gitignore");

        if !gitignore_path.exists() {
            return Ok(None);
        }

        let mut builder = GitignoreBuilder::new(base_path);
        builder.add(&gitignore_path);

        match builder.build() {
            Ok(gitignore) => Ok(Some(gitignore)),
            Err(e) => {
                tracing::warn!("Failed to parse .gitignore: {}", e);
                Ok(None)
            }
        }
    }

    pub async fn sync(
        &self,
        source: &Path,
        destination: &Path,
    ) -> Result<SyncResult, Box<dyn std::error::Error>> {
        let start = std::time::Instant::now();

        let mut result = SyncResult {
            files_copied: vec![],
            files_updated: vec![],
            files_deleted: vec![],
            files_skipped: vec![],
            bytes_transferred: 0,
            duration_ms: 0,
            dry_run: self.options.dry_run,
        };

        // Scan source directory
        let source_files = self.scan_directory(source).await?;

        // Scan destination directory
        let dest_files = if destination.exists() {
            self.scan_directory(destination).await?
        } else {
            HashMap::new()
        };

        // Process source files
        for (relative_path, source_info) in &source_files {
            if self.should_skip(&relative_path) {
                result.files_skipped.push(relative_path.clone());
                continue;
            }

            let dest_path = destination.join(&relative_path);

            if let Some(dest_info) = dest_files.get(relative_path) {
                // File exists in destination
                if self.should_update(&source_info, &dest_info).await? {
                    if !self.options.dry_run {
                        self.copy_file(&source_info.path, &dest_path).await?;
                        result.bytes_transferred += source_info.size;
                    }
                    result.files_updated.push(relative_path.clone());
                }
            } else {
                // File doesn't exist in destination
                if !self.options.dry_run {
                    if let Some(parent) = dest_path.parent() {
                        fs::create_dir_all(parent).await?;
                    }
                    self.copy_file(&source_info.path, &dest_path).await?;
                    result.bytes_transferred += source_info.size;
                }
                result.files_copied.push(relative_path.clone());
            }
        }

        // Handle deletion of unmatched files
        if self.options.delete_unmatched {
            for (relative_path, _) in &dest_files {
                if !source_files.contains_key(relative_path) && !self.should_skip(relative_path) {
                    let dest_path = destination.join(&relative_path);
                    if !self.options.dry_run {
                        fs::remove_file(dest_path).await?;
                    }
                    result.files_deleted.push(relative_path.clone());
                }
            }
        }

        result.duration_ms = start.elapsed().as_millis() as u64;
        Ok(result)
    }

    async fn scan_directory(
        &self,
        path: &Path,
    ) -> Result<HashMap<String, FileInfo>, Box<dyn std::error::Error>> {
        let mut files = HashMap::new();
        self.scan_recursive(path, path, &mut files).await?;
        Ok(files)
    }

    async fn scan_recursive(
        &self,
        base: &Path,
        current: &Path,
        files: &mut HashMap<String, FileInfo>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut entries = fs::read_dir(current).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let metadata = entry.metadata().await?;

            if metadata.is_file() {
                let relative_path = path.strip_prefix(base)?.to_string_lossy().to_string();

                // Skip files that should be ignored according to gitignore or filters
                if self.should_skip(&relative_path) {
                    continue;
                }

                let checksum = if self.options.checksum_only {
                    Some(self.calculate_checksum(&path).await?)
                } else {
                    None
                };

                let modified = DateTime::from_timestamp(
                    metadata
                        .modified()?
                        .duration_since(std::time::UNIX_EPOCH)?
                        .as_secs() as i64,
                    0,
                )
                .unwrap_or_else(|| Utc::now());

                files.insert(
                    relative_path,
                    FileInfo {
                        path: path.clone(),
                        size: metadata.len(),
                        modified,
                        checksum,
                    },
                );
            } else if metadata.is_dir() && !self.is_ignored(&path) {
                Box::pin(self.scan_recursive(base, &path, files)).await?;
            }
        }

        Ok(())
    }

    fn should_skip(&self, path: &str) -> bool {
        // Skip .gitignore file itself
        if path == ".gitignore" {
            return true;
        }

        // Check gitignore
        if let Some(ref gitignore) = self.gitignore {
            if gitignore.matched(path, false).is_ignore() {
                return true;
            }
        }

        // Check exclude patterns
        for pattern in &self.options.exclude_patterns {
            if path.contains(pattern)
                || glob::Pattern::new(pattern)
                    .map(|p| p.matches(path))
                    .unwrap_or(false)
            {
                return true;
            }
        }

        // Check include patterns (if specified, only include matching files)
        if !self.options.include_patterns.is_empty() {
            let included = self.options.include_patterns.iter().any(|pattern| {
                path.contains(pattern)
                    || glob::Pattern::new(pattern)
                        .map(|p| p.matches(path))
                        .unwrap_or(false)
            });
            return !included;
        }

        false
    }

    fn is_ignored(&self, path: &Path) -> bool {
        if let Some(file_name) = path.file_name() {
            let name = file_name.to_string_lossy();
            // Common directories to ignore
            matches!(
                name.as_ref(),
                ".git" | "node_modules" | ".DS_Store" | "target"
            )
        } else {
            false
        }
    }

    async fn should_update(
        &self,
        source: &FileInfo,
        dest: &FileInfo,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        if self.options.checksum_only {
            // Compare checksums
            let source_checksum = if let Some(ref sum) = source.checksum {
                sum.clone()
            } else {
                self.calculate_checksum(&source.path).await?
            };

            let dest_checksum = if let Some(ref sum) = dest.checksum {
                sum.clone()
            } else {
                self.calculate_checksum(&dest.path).await?
            };

            Ok(source_checksum != dest_checksum)
        } else {
            // Compare size and modification time
            Ok(source.size != dest.size || source.modified > dest.modified)
        }
    }

    async fn calculate_checksum(&self, path: &Path) -> Result<String, Box<dyn std::error::Error>> {
        let mut file = fs::File::open(path).await?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; 8192];

        loop {
            let n = file.read(&mut buffer).await?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }

        Ok(format!("{:x}", hasher.finalize()))
    }

    async fn copy_file(
        &self,
        source: &Path,
        dest: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        fs::copy(source, dest).await?;

        if self.options.preserve_timestamps {
            let metadata = fs::metadata(source).await?;
            if let Ok(modified) = metadata.modified() {
                // Note: Setting file times requires platform-specific code
                // This is a simplified version
                filetime::set_file_mtime(dest, filetime::FileTime::from_system_time(modified))?;
            }
        }

        Ok(())
    }
}

use std::sync::Arc;

/// Instance file synchronization
pub struct InstanceSync {
    instance_id: String,
    container_id: String,
}

impl InstanceSync {
    pub fn new(instance_id: String, container_id: String) -> Self {
        Self {
            instance_id,
            container_id,
        }
    }

    pub async fn sync_to_instance(
        &self,
        local_dir: &Path,
        remote_dir: &str,
        options: SyncOptions,
    ) -> Result<SyncResult, Box<dyn std::error::Error>> {
        let synchronizer = FileSynchronizer::new(local_dir, options.clone()).await?;

        // Create temporary directory for staging
        let temp_dir = tempfile::tempdir()?;
        let staging_path = temp_dir.path();

        // Sync to staging
        let result = synchronizer.sync(local_dir, staging_path).await?;

        if !options.dry_run {
            // Package and upload to instance
            let tar_data = self.create_tar_archive(staging_path).await?;
            // Would call Docker/executor API to copy files
            // For now, simulate copy operation
            tracing::info!(
                "Would copy files to container {} at {}",
                self.container_id,
                remote_dir
            );
        }

        Ok(result)
    }

    pub async fn sync_from_instance(
        &self,
        remote_dir: &str,
        local_dir: &Path,
        options: SyncOptions,
    ) -> Result<SyncResult, Box<dyn std::error::Error>> {
        // Download from instance
        // Would call Docker/executor API to copy files
        // For now, simulate download operation
        tracing::info!(
            "Would copy files from container {} at {}",
            self.container_id,
            remote_dir
        );
        let tar_data = vec![];

        // Extract to temporary directory
        let temp_dir = tempfile::tempdir()?;
        let staging_path = temp_dir.path();
        self.extract_tar_archive(&tar_data, staging_path).await?;

        // Sync from staging to local
        let synchronizer = FileSynchronizer::new(staging_path, options).await?;
        synchronizer.sync(staging_path, local_dir).await
    }

    async fn create_tar_archive(&self, path: &Path) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let mut archive = tar::Builder::new(Vec::new());
        archive.append_dir_all(".", path)?;
        Ok(archive.into_inner()?)
    }

    async fn extract_tar_archive(
        &self,
        data: &[u8],
        dest: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut archive = tar::Archive::new(std::io::Cursor::new(data));
        archive.unpack(dest)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_basic_sync() {
        let source_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();

        // Create test files in source
        let file1 = source_dir.path().join("file1.txt");
        fs::write(&file1, b"content1").await.unwrap();

        let subdir = source_dir.path().join("subdir");
        fs::create_dir(&subdir).await.unwrap();
        let file2 = subdir.join("file2.txt");
        fs::write(&file2, b"content2").await.unwrap();

        // Perform sync
        let options = SyncOptions::default();
        let synchronizer = FileSynchronizer::new(source_dir.path(), options)
            .await
            .unwrap();
        let result = synchronizer
            .sync(source_dir.path(), dest_dir.path())
            .await
            .unwrap();

        assert_eq!(result.files_copied.len(), 2);
        assert!(dest_dir.path().join("file1.txt").exists());
        assert!(dest_dir.path().join("subdir/file2.txt").exists());
    }

    #[tokio::test]
    async fn test_gitignore_filtering() {
        let source_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();

        // Create .gitignore
        let gitignore = source_dir.path().join(".gitignore");
        fs::write(&gitignore, "*.log\ntemp/**\n").await.unwrap();

        // Create files
        fs::write(source_dir.path().join("keep.txt"), b"keep")
            .await
            .unwrap();
        fs::write(source_dir.path().join("skip.log"), b"skip")
            .await
            .unwrap();

        let temp_dir = source_dir.path().join("temp");
        fs::create_dir(&temp_dir).await.unwrap();
        fs::write(temp_dir.join("temp.txt"), b"temp").await.unwrap();

        // Sync with gitignore
        let options = SyncOptions {
            use_gitignore: true,
            ..Default::default()
        };

        let synchronizer = FileSynchronizer::new(source_dir.path(), options)
            .await
            .unwrap();
        let result = synchronizer
            .sync(source_dir.path(), dest_dir.path())
            .await
            .unwrap();

        assert_eq!(result.files_copied.len(), 1);
        assert!(dest_dir.path().join("keep.txt").exists());
        assert!(!dest_dir.path().join("skip.log").exists());
        assert!(!dest_dir.path().join("temp/temp.txt").exists());
    }

    #[tokio::test]
    async fn test_checksum_comparison() {
        let source_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();

        let file = source_dir.path().join("file.txt");
        fs::write(&file, b"content").await.unwrap();

        // First sync
        let options = SyncOptions {
            checksum_only: true,
            ..Default::default()
        };

        let synchronizer = FileSynchronizer::new(source_dir.path(), options.clone())
            .await
            .unwrap();
        let result1 = synchronizer
            .sync(source_dir.path(), dest_dir.path())
            .await
            .unwrap();
        assert_eq!(result1.files_copied.len(), 1);

        // Second sync with same content
        let result2 = synchronizer
            .sync(source_dir.path(), dest_dir.path())
            .await
            .unwrap();
        assert_eq!(result2.files_updated.len(), 0);

        // Update file
        fs::write(&file, b"new content").await.unwrap();
        let result3 = synchronizer
            .sync(source_dir.path(), dest_dir.path())
            .await
            .unwrap();
        assert_eq!(result3.files_updated.len(), 1);
    }

    #[tokio::test]
    async fn test_dry_run() {
        let source_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();

        fs::write(source_dir.path().join("file.txt"), b"content")
            .await
            .unwrap();

        let options = SyncOptions {
            dry_run: true,
            ..Default::default()
        };

        let synchronizer = FileSynchronizer::new(source_dir.path(), options)
            .await
            .unwrap();
        let result = synchronizer
            .sync(source_dir.path(), dest_dir.path())
            .await
            .unwrap();

        assert!(result.dry_run);
        assert_eq!(result.files_copied.len(), 1);
        assert!(!dest_dir.path().join("file.txt").exists());
    }
}
