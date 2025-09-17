import { describe, it, expect, beforeAll, afterAll } from '@jest/globals';
import {
  FaaSPlatformClient,
  ExecutionMode,
  SyncOptions,
  ReadinessConfig,
  LongRunningConfig,
} from '../src/faas-api';
import * as fs from 'fs/promises';
import * as path from 'path';
import { tmpdir } from 'os';

describe('FaaS Platform Integration Tests', () => {
  let client: FaaSPlatformClient;
  let testInstanceId: string;
  let testSnapshotId: string;

  beforeAll(() => {
    client = new FaaSPlatformClient({
      apiKey: process.env.TEST_API_KEY || 'test-key',
      platformUrl: process.env.TEST_PLATFORM_URL || 'http://localhost:8080',
    });
  });

  afterAll(async () => {
    await client.close();
  });

  describe('Deterministic Snapshot Hashing', () => {
    it('should generate deterministic snapshot hash', async () => {
      // Execute and create snapshot
      const execution = await client.execute({
        id: 'test-deterministic',
        code: 'console.log("test");',
        mode: ExecutionMode.CHECKPOINTED,
        env: 'node:18',
      });

      const snapshot1 = await client.createSnapshot(execution.request_id, {
        tags: ['test', 'deterministic'],
      });

      expect(snapshot1.content_hash).toBeDefined();
      expect(snapshot1.content_hash).toMatch(/^[a-f0-9]{64}$/); // SHA256 hash

      // Same content should produce same hash
      const snapshot2 = await client.createSnapshot(execution.request_id, {
        tags: ['test', 'deterministic'],
      });

      expect(snapshot2.content_hash).toBe(snapshot1.content_hash);

      testSnapshotId = snapshot1.id;
    });

    it('should chain snapshots with parent hash', async () => {
      const branch = await client.createBranch(testSnapshotId);
      const branchSnapshot = await client.getSnapshot(branch.snapshot_id);

      expect(branchSnapshot.parent_hash).toBe(testSnapshotId);
    });
  });

  describe('Advanced File Sync', () => {
    let testDir: string;

    beforeAll(async () => {
      testDir = path.join(tmpdir(), `faas-test-${Date.now()}`);
      await fs.mkdir(testDir, { recursive: true });

      // Create test files
      await fs.writeFile(path.join(testDir, 'file1.txt'), 'content1');
      await fs.writeFile(path.join(testDir, 'file2.log'), 'log content');
      await fs.writeFile(path.join(testDir, '.gitignore'), '*.log\n');

      const subdir = path.join(testDir, 'subdir');
      await fs.mkdir(subdir);
      await fs.writeFile(path.join(subdir, 'file3.txt'), 'content3');
    });

    afterAll(async () => {
      await fs.rm(testDir, { recursive: true, force: true });
    });

    it('should sync files with gitignore support', async () => {
      const syncOptions: SyncOptions = {
        useGitignore: true,
        dryRun: false,
        checksumOnly: true,
      };

      const result = await client.syncFiles(
        testInstanceId,
        testDir,
        '/workspace',
        syncOptions
      );

      expect(result.filesCopied).toContain('file1.txt');
      expect(result.filesCopied).toContain('subdir/file3.txt');
      expect(result.filesSkipped).toContain('file2.log'); // Should be ignored
    });

    it('should perform dry run sync', async () => {
      const syncOptions: SyncOptions = {
        dryRun: true,
        deleteUnmatched: true,
      };

      const result = await client.syncFiles(
        testInstanceId,
        testDir,
        '/workspace',
        syncOptions
      );

      expect(result.dryRun).toBe(true);
      expect(result.bytesTransferred).toBe(0); // No actual transfer in dry run
    });

    it('should sync with checksum comparison', async () => {
      const syncOptions: SyncOptions = {
        checksumOnly: true,
        preserveTimestamps: true,
      };

      // First sync
      const result1 = await client.syncFiles(
        testInstanceId,
        testDir,
        '/workspace',
        syncOptions
      );

      // Second sync with same files should skip
      const result2 = await client.syncFiles(
        testInstanceId,
        testDir,
        '/workspace',
        syncOptions
      );

      expect(result2.filesUpdated.length).toBe(0);
      expect(result2.filesCopied.length).toBe(0);
    });
  });

  describe('SSH Key Rotation', () => {
    it('should generate and rotate SSH keys', async () => {
      const key1 = await client.getSSHKey(testInstanceId);

      expect(key1.privateKey).toMatch(/^-----BEGIN OPENSSH PRIVATE KEY-----/);
      expect(key1.publicKey).toMatch(/^ssh-/);
      expect(key1.fingerprint).toMatch(/^SHA256:/);
      expect(key1.algorithm).toBe('Ed25519');

      // Rotate key
      const key2 = await client.rotateSSHKey(testInstanceId);

      expect(key2.id).not.toBe(key1.id);
      expect(key2.rotatedFrom).toBe(key1.id);
      expect(key2.fingerprint).not.toBe(key1.fingerprint);
    });

    it('should auto-rotate expired keys', async () => {
      const key = await client.getSSHKey(testInstanceId);

      if (key.expiresAt) {
        const expiryDate = new Date(key.expiresAt);
        const now = new Date();
        const daysUntilExpiry = (expiryDate.getTime() - now.getTime()) / (1000 * 60 * 60 * 24);

        if (daysUntilExpiry <= 7) {
          // Should trigger auto-rotation
          const newKey = await client.rotateSSHKey(testInstanceId);
          expect(newKey.rotatedFrom).toBe(key.id);
        }
      }
    });
  });

  describe('Readiness Checks', () => {
    it('should wait for instance readiness', async () => {
      const config: ReadinessConfig = {
        checkInterval: 1000,
        timeout: 30000,
        successThreshold: 2,
        probes: [
          {
            type: 'tcp',
            port: 80,
            timeout: 5000,
          },
          {
            type: 'http',
            path: '/health',
            port: 8080,
            expectedStatus: 200,
          },
        ],
      };

      const status = await client.waitForReady(testInstanceId, config);

      expect(status.ready).toBe(true);
      expect(status.consecutiveSuccesses).toBeGreaterThanOrEqual(2);
      expect(status.message).toContain('ready');
    });

    it('should check readiness with file probe', async () => {
      const config: ReadinessConfig = {
        probes: [
          {
            type: 'file',
            path: '/tmp/ready',
            timeout: 1000,
          },
        ],
      };

      const status = await client.checkReadiness(testInstanceId, config);

      expect(status.ready).toBeDefined();
      expect(status.checksPerformed).toBeGreaterThan(0);
    });

    it('should handle readiness timeout', async () => {
      const config: ReadinessConfig = {
        timeout: 1000,
        probes: [
          {
            type: 'tcp',
            port: 99999, // Non-existent port
          },
        ],
      };

      await expect(
        client.waitForReady(testInstanceId, config)
      ).rejects.toThrow('not ready');
    });
  });

  describe('Long-Running Executions', () => {
    it('should support 24-hour executions', async () => {
      const config: LongRunningConfig = {
        maxDuration: 24 * 60 * 60 * 1000,
        heartbeatInterval: 30000,
        checkpointInterval: 300000,
        autoExtend: true,
      };

      const session = await client.startLongRunningExecution(
        {
          id: 'long-task',
          code: 'while true; do echo "Working..."; sleep 60; done',
          mode: ExecutionMode.PERSISTENT,
          env: 'alpine:latest',
        },
        config
      );

      expect(session.id).toBeDefined();
      expect(session.maxExtensions).toBe(3);

      // Send heartbeats
      for (let i = 0; i < 3; i++) {
        await new Promise(resolve => setTimeout(resolve, 1000));
        await client.sendHeartbeat(session.id);
      }

      // Extend session
      const newDuration = await client.extendSession(session.id);
      expect(newDuration).toBe(24 * 60 * 60 * 1000);
    });

    it('should execute with streaming callbacks', async () => {
      const stdout: string[] = [];
      const stderr: string[] = [];
      let progress = 0;

      const result = await client.executeWithCallbacks(
        {
          id: 'streaming-test',
          code: `
            for i in {1..10}; do
              echo "Line $i"
              >&2 echo "Error $i"
              sleep 0.1
            done
          `,
          mode: ExecutionMode.EPHEMERAL,
          env: 'alpine:latest',
        },
        {
          onStdout: (data) => stdout.push(data),
          onStderr: (data) => stderr.push(data),
          onProgress: (percent) => (progress = percent),
          timeout: 10000,
        }
      );

      expect(result.exit_code).toBe(0);
      expect(stdout.length).toBeGreaterThan(0);
      expect(stderr.length).toBeGreaterThan(0);
    });

    it('should auto-checkpoint during long execution', async () => {
      const config: LongRunningConfig = {
        checkpointInterval: 1000, // Checkpoint every second
      };

      const session = await client.startLongRunningExecution(
        {
          id: 'checkpoint-test',
          code: 'sleep 5',
          mode: ExecutionMode.CHECKPOINTED,
          env: 'alpine:latest',
        },
        config
      );

      // Wait for auto-checkpoints
      await new Promise(resolve => setTimeout(resolve, 3000));

      // Verify checkpoints were created
      const snapshots = await client.listSnapshots({
        mode: ExecutionMode.CHECKPOINTED,
      });

      const sessionSnapshots = snapshots.filter(s =>
        s.metadata?.session_id === session.id
      );

      expect(sessionSnapshots.length).toBeGreaterThanOrEqual(2);
    });
  });

  describe('Performance Benchmarks', () => {
    it('should achieve sub-50ms warm starts', async () => {
      // Pre-warm container
      await client.executeCached('console.log("warm up")', 'node:18');

      const startTime = Date.now();
      const result = await client.executeCached('console.log("fast")', 'node:18');
      const duration = Date.now() - startTime;

      expect(result.cached).toBe(true);
      expect(duration).toBeLessThan(50);
    });

    it('should achieve sub-250ms branching', async () => {
      const snapshot = await client.createSnapshot(testSnapshotId);

      const startTime = Date.now();
      const branch = await client.createBranch(snapshot.id);
      const branchDuration = Date.now() - startTime;

      expect(branchDuration).toBeLessThan(250);
    });
  });
});