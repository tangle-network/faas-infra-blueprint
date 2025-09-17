use chrono::{DateTime, Duration, Utc};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use ssh_key::private::Ed25519Keypair;
use ssh_key::{Algorithm, LineEnding, PrivateKey, PublicKey};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::process::Command;

/// SSH key management with automatic rotation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshKeyPair {
    pub id: String,
    pub private_key: String,
    pub public_key: String,
    pub fingerprint: String,
    pub algorithm: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub rotated_from: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshConfig {
    pub auto_rotate: bool,
    pub rotation_interval: Duration,
    pub key_algorithm: SshKeyAlgorithm,
    pub max_keys_per_instance: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SshKeyAlgorithm {
    Ed25519,
    Rsa4096,
    EcdsaP256,
}

impl Default for SshConfig {
    fn default() -> Self {
        Self {
            auto_rotate: true,
            rotation_interval: Duration::days(30),
            key_algorithm: SshKeyAlgorithm::Ed25519,
            max_keys_per_instance: 3,
        }
    }
}

pub struct SshKeyManager {
    keys_dir: PathBuf,
    config: SshConfig,
    active_keys: HashMap<String, Vec<SshKeyPair>>,
}

impl SshKeyManager {
    pub fn new(keys_dir: impl AsRef<Path>, config: SshConfig) -> Self {
        Self {
            keys_dir: keys_dir.as_ref().to_path_buf(),
            config,
            active_keys: HashMap::new(),
        }
    }

    /// Generate new SSH key pair
    pub async fn generate_keypair(&self) -> Result<SshKeyPair, Box<dyn std::error::Error>> {
        let key_id = self.generate_key_id();

        let (private_key, public_key, fingerprint) = match self.config.key_algorithm {
            SshKeyAlgorithm::Ed25519 => self.generate_ed25519().await?,
            SshKeyAlgorithm::Rsa4096 => self.generate_rsa().await?,
            SshKeyAlgorithm::EcdsaP256 => self.generate_ecdsa().await?,
        };

        let expires_at = if self.config.auto_rotate {
            Some(Utc::now() + self.config.rotation_interval)
        } else {
            None
        };

        let keypair = SshKeyPair {
            id: key_id,
            private_key,
            public_key,
            fingerprint,
            algorithm: format!("{:?}", self.config.key_algorithm),
            created_at: Utc::now(),
            expires_at,
            rotated_from: None,
        };

        // Store key files
        self.store_keypair(&keypair).await?;

        Ok(keypair)
    }

    async fn generate_ed25519(
        &self,
    ) -> Result<(String, String, String), Box<dyn std::error::Error>> {
        let rng = SystemRandom::new();
        let mut seed = [0u8; 32];
        rng.fill(&mut seed).map_err(|e| format!("Failed to generate random seed: {:?}", e))?;

        let keypair = Ed25519Keypair::from_seed(&seed);
        let private_key = PrivateKey::from(keypair);
        let public_key = private_key.public_key();

        let private_pem = private_key.to_openssh(LineEnding::LF)?;
        let public_str = public_key.to_openssh()?;
        let fingerprint = self.calculate_fingerprint(&public_key)?;

        Ok((private_pem.to_string(), public_str, fingerprint))
    }

    async fn generate_rsa(&self) -> Result<(String, String, String), Box<dyn std::error::Error>> {
        // Use ssh-keygen for RSA keys
        let output = Command::new("ssh-keygen")
            .args(&["-t", "rsa", "-b", "4096", "-N", "", "-f", "-"])
            .output()
            .await?;

        if !output.status.success() {
            return Err("Failed to generate RSA key".into());
        }

        let private_key = String::from_utf8(output.stdout)?;

        // Extract public key
        let pub_output = Command::new("ssh-keygen")
            .args(&["-y", "-f", "-"])
            .arg(&private_key)
            .output()
            .await?;

        let public_key = String::from_utf8(pub_output.stdout)?;
        let fingerprint = self.calculate_fingerprint_from_string(&public_key)?;

        Ok((private_key, public_key, fingerprint))
    }

    async fn generate_ecdsa(&self) -> Result<(String, String, String), Box<dyn std::error::Error>> {
        let output = Command::new("ssh-keygen")
            .args(&["-t", "ecdsa", "-b", "256", "-N", "", "-f", "-"])
            .output()
            .await?;

        if !output.status.success() {
            return Err("Failed to generate ECDSA key".into());
        }

        let private_key = String::from_utf8(output.stdout)?;

        let pub_output = Command::new("ssh-keygen")
            .args(&["-y", "-f", "-"])
            .arg(&private_key)
            .output()
            .await?;

        let public_key = String::from_utf8(pub_output.stdout)?;
        let fingerprint = self.calculate_fingerprint_from_string(&public_key)?;

        Ok((private_key, public_key, fingerprint))
    }

    /// Rotate SSH key for instance
    pub async fn rotate_key(
        &mut self,
        instance_id: &str,
        current_key_id: &str,
    ) -> Result<SshKeyPair, Box<dyn std::error::Error>> {
        // Generate new key
        let mut new_key = self.generate_keypair().await?;
        new_key.rotated_from = Some(current_key_id.to_string());

        // Get instance keys
        let instance_keys = self
            .active_keys
            .entry(instance_id.to_string())
            .or_insert_with(Vec::new);

        // Add new key
        instance_keys.push(new_key.clone());

        // Remove old keys if exceeding limit
        if instance_keys.len() > self.config.max_keys_per_instance {
            let to_remove = instance_keys.len() - self.config.max_keys_per_instance;
            let mut keys_to_revoke = Vec::new();

            for _ in 0..to_remove {
                if let Some(old_key) = instance_keys.first() {
                    keys_to_revoke.push(old_key.id.clone());
                    instance_keys.remove(0);
                }
            }

            // Revoke keys after borrowing ends
            for key_id in keys_to_revoke {
                self.revoke_key(&key_id).await?;
            }
        }

        Ok(new_key)
    }

    /// Check if key needs rotation
    pub fn needs_rotation(&self, keypair: &SshKeyPair) -> bool {
        if !self.config.auto_rotate {
            return false;
        }

        if let Some(expires_at) = keypair.expires_at {
            Utc::now() >= expires_at - Duration::days(7) // Rotate 7 days before expiry
        } else {
            false
        }
    }

    /// Revoke SSH key
    pub async fn revoke_key(&self, key_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        let key_dir = self.keys_dir.join(key_id);

        if key_dir.exists() {
            // Move to revoked directory instead of deleting
            let revoked_dir = self.keys_dir.join("revoked").join(key_id);
            fs::create_dir_all(revoked_dir.parent().unwrap()).await?;
            fs::rename(&key_dir, &revoked_dir).await?;
        }

        Ok(())
    }

    /// Add SSH key to instance authorized_keys
    pub async fn authorize_key(
        &self,
        instance_id: &str,
        keypair: &SshKeyPair,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let instance_dir = format!("/tmp/instances/{}", instance_id);
        let authorized_keys_path = format!("{}/authorized_keys", instance_dir);

        // Create instance directory if it doesn't exist
        fs::create_dir_all(&instance_dir).await?;

        // Read existing keys
        let mut authorized_keys = if Path::new(&authorized_keys_path).exists() {
            fs::read_to_string(&authorized_keys_path).await?
        } else {
            String::new()
        };

        // Add new key if not already present
        if !authorized_keys.contains(&keypair.public_key) {
            authorized_keys.push_str(&format!("{}\n", keypair.public_key));
            fs::write(&authorized_keys_path, authorized_keys).await?;
        }

        Ok(())
    }

    /// Remove SSH key from instance authorized_keys
    pub async fn deauthorize_key(
        &self,
        instance_id: &str,
        key_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let authorized_keys_path = format!("/tmp/instances/{}/authorized_keys", instance_id);

        if !Path::new(&authorized_keys_path).exists() {
            return Ok(());
        }

        let keypair = self.load_keypair(key_id).await?;
        let authorized_keys = fs::read_to_string(&authorized_keys_path).await?;

        let updated_keys: Vec<&str> = authorized_keys
            .lines()
            .filter(|line| !line.contains(&keypair.public_key))
            .collect();

        fs::write(&authorized_keys_path, updated_keys.join("\n")).await?;
        Ok(())
    }

    async fn store_keypair(&self, keypair: &SshKeyPair) -> Result<(), Box<dyn std::error::Error>> {
        let key_dir = self.keys_dir.join(&keypair.id);
        fs::create_dir_all(&key_dir).await?;

        // Store private key
        let private_path = key_dir.join("id_ed25519");
        fs::write(&private_path, &keypair.private_key).await?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&private_path).await?.permissions();
            perms.set_mode(0o600);
            fs::set_permissions(&private_path, perms).await?;
        }

        // Store public key
        let public_path = key_dir.join("id_ed25519.pub");
        fs::write(&public_path, &keypair.public_key).await?;

        // Store metadata
        let metadata_path = key_dir.join("metadata.json");
        let metadata_json = serde_json::to_vec_pretty(keypair)?;
        fs::write(&metadata_path, metadata_json).await?;

        Ok(())
    }

    async fn load_keypair(&self, key_id: &str) -> Result<SshKeyPair, Box<dyn std::error::Error>> {
        let metadata_path = self.keys_dir.join(key_id).join("metadata.json");
        let metadata_json = fs::read(&metadata_path).await?;
        Ok(serde_json::from_slice(&metadata_json)?)
    }

    fn generate_key_id(&self) -> String {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        format!("key_{:016x}", rng.gen::<u64>())
    }

    fn calculate_fingerprint(
        &self,
        public_key: &PublicKey,
    ) -> Result<String, Box<dyn std::error::Error>> {
        use sha2::{Digest, Sha256};
        let key_data = public_key.to_bytes()?;
        let hash = Sha256::digest(&key_data);
        Ok(format!("SHA256:{}", base64::encode(hash)))
    }

    fn calculate_fingerprint_from_string(
        &self,
        public_key: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(public_key.as_bytes());
        Ok(format!("SHA256:{}", base64::encode(hash)))
    }
}

/// SSH connection manager
pub struct SshConnectionManager {
    connections: HashMap<String, SshConnection>,
    key_manager: SshKeyManager,
}

#[derive(Clone)]
struct SshConnection {
    instance_id: String,
    host: String,
    port: u16,
    keypair: SshKeyPair,
    established_at: DateTime<Utc>,
}

impl SshConnectionManager {
    pub fn new(key_manager: SshKeyManager) -> Self {
        Self {
            connections: HashMap::new(),
            key_manager,
        }
    }

    pub async fn connect(
        &mut self,
        instance_id: &str,
        host: &str,
        port: u16,
    ) -> Result<SshKeyPair, Box<dyn std::error::Error>> {
        // Check for existing connection
        if let Some(conn) = self.connections.get(instance_id) {
            // Check if key needs rotation
            if self.key_manager.needs_rotation(&conn.keypair) {
                let new_key = self
                    .key_manager
                    .rotate_key(instance_id, &conn.keypair.id)
                    .await?;
                self.key_manager
                    .authorize_key(instance_id, &new_key)
                    .await?;

                let mut updated_conn = conn.clone();
                updated_conn.keypair = new_key.clone();
                self.connections
                    .insert(instance_id.to_string(), updated_conn);

                return Ok(new_key);
            }
            return Ok(conn.keypair.clone());
        }

        // Generate new keypair
        let keypair = self.key_manager.generate_keypair().await?;
        self.key_manager
            .authorize_key(instance_id, &keypair)
            .await?;

        self.connections.insert(
            instance_id.to_string(),
            SshConnection {
                instance_id: instance_id.to_string(),
                host: host.to_string(),
                port,
                keypair: keypair.clone(),
                established_at: Utc::now(),
            },
        );

        Ok(keypair)
    }

    pub async fn disconnect(
        &mut self,
        instance_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(conn) = self.connections.remove(instance_id) {
            self.key_manager
                .deauthorize_key(instance_id, &conn.keypair.id)
                .await?;
            self.key_manager.revoke_key(&conn.keypair.id).await?;
        }
        Ok(())
    }

    pub async fn rotate_all_expiring_keys(
        &mut self,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let mut rotated = Vec::new();

        for (instance_id, conn) in &self.connections {
            if self.key_manager.needs_rotation(&conn.keypair) {
                let new_key = self
                    .key_manager
                    .rotate_key(instance_id, &conn.keypair.id)
                    .await?;
                self.key_manager
                    .authorize_key(instance_id, &new_key)
                    .await?;
                rotated.push(instance_id.clone());
            }
        }

        Ok(rotated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_keypair_generation() {
        let temp_dir = tempdir().unwrap();
        let config = SshConfig::default();
        let manager = SshKeyManager::new(temp_dir.path(), config);

        let keypair = manager.generate_keypair().await.unwrap();

        assert!(!keypair.private_key.is_empty());
        assert!(!keypair.public_key.is_empty());
        assert!(!keypair.fingerprint.is_empty());
        assert_eq!(keypair.algorithm, "Ed25519");
    }

    #[tokio::test]
    async fn test_key_rotation() {
        let temp_dir = tempdir().unwrap();
        let config = SshConfig {
            auto_rotate: true,
            rotation_interval: Duration::days(1),
            ..Default::default()
        };

        let mut manager = SshKeyManager::new(temp_dir.path(), config);

        let original = manager.generate_keypair().await.unwrap();
        let rotated = manager.rotate_key("instance1", &original.id).await.unwrap();

        assert_ne!(original.id, rotated.id);
        assert_eq!(rotated.rotated_from, Some(original.id));
    }

    #[tokio::test]
    async fn test_key_authorization() {
        let temp_dir = tempdir().unwrap();
        let config = SshConfig::default();
        let manager = SshKeyManager::new(temp_dir.path(), config);

        let keypair = manager.generate_keypair().await.unwrap();

        // Create mock instance directory
        let instance_dir = temp_dir.path().join("instances").join("test-instance");
        fs::create_dir_all(&instance_dir).await.unwrap();

        manager
            .authorize_key("test-instance", &keypair)
            .await
            .unwrap();

        // Verify key was added
        let auth_keys_path = format!("/tmp/instances/test-instance/authorized_keys");
        if Path::new(&auth_keys_path).exists() {
            let content = fs::read_to_string(&auth_keys_path).await.unwrap();
            assert!(content.contains(&keypair.public_key));
        }
    }

    #[tokio::test]
    async fn test_connection_management() {
        let temp_dir = tempdir().unwrap();
        let key_manager = SshKeyManager::new(temp_dir.path(), SshConfig::default());
        let mut conn_manager = SshConnectionManager::new(key_manager);

        let key1 = conn_manager
            .connect("inst1", "localhost", 22)
            .await
            .unwrap();
        let key2 = conn_manager
            .connect("inst1", "localhost", 22)
            .await
            .unwrap();

        assert_eq!(key1.id, key2.id); // Same instance should reuse key

        conn_manager.disconnect("inst1").await.unwrap();
    }
}
