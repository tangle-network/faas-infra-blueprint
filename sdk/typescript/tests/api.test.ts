/**
 * FaaS Platform API Tests
 */

import { describe, it, expect, beforeEach, afterEach, jest } from '@jest/globals';
import fetch from 'node-fetch';
import {
  FaaSClient,
  FaaSError,
  TimeoutError,
  InstanceState,
  Snapshot,
  Instance,
  ExecutionResult,
} from '../src/api';

jest.mock('node-fetch');
const mockFetch = fetch as jest.MockedFunction<typeof fetch>;

describe('FaaSClient', () => {
  let client: FaaSClient;
  const mockApiKey = 'test-api-key';
  const mockBaseUrl = 'http://localhost:8080/api';

  beforeEach(() => {
    client = new FaaSClient({
      apiKey: mockApiKey,
      baseUrl: mockBaseUrl,
      timeout: 5000,
      maxRetries: 2,
    });
  });

  afterEach(() => {
    jest.clearAllMocks();
  });

  describe('constructor', () => {
    it('should create client with provided options', () => {
      expect(client).toBeInstanceOf(FaaSClient);
    });

    it('should throw error if no API key provided', () => {
      expect(() => new FaaSClient({ apiKey: '' })).toThrow('API key is required');
    });

    it('should use environment variable for API key', () => {
      process.env.FAAS_API_KEY = 'env-api-key';
      const envClient = new FaaSClient();
      expect(envClient).toBeInstanceOf(FaaSClient);
      delete process.env.FAAS_API_KEY;
    });
  });

  describe('Snapshot Operations', () => {
    const mockSnapshot: Snapshot = {
      id: 'snap-123',
      parent_id: 'snap-parent',
      created_at: '2024-01-01T00:00:00Z',
      size: 1024,
      checksum: 'abc123',
      metadata: { name: 'test-snapshot' },
    };

    describe('createSnapshot', () => {
      it('should create snapshot with options', async () => {
        mockFetch.mockResolvedValueOnce({
          ok: true,
          json: async () => mockSnapshot,
        } as any);

        const result = await client.createSnapshot({
          base_image: 'ubuntu:22.04',
          resource_spec: { vcpus: 2, memory: 1024, disk_size: 10240 },
        });

        expect(result).toEqual(mockSnapshot);
        expect(mockFetch).toHaveBeenCalledWith(
          `${mockBaseUrl}/snapshots`,
          expect.objectContaining({
            method: 'POST',
            headers: expect.objectContaining({
              'Authorization': `Bearer ${mockApiKey}`,
              'Content-Type': 'application/json',
            }),
            body: expect.stringContaining('ubuntu:22.04'),
          })
        );
      });

      it('should handle creation errors', async () => {
        mockFetch.mockResolvedValueOnce({
          ok: false,
          status: 400,
          json: async () => ({ message: 'Invalid resource spec' }),
        } as any);

        await expect(
          client.createSnapshot({ base_image: 'invalid' })
        ).rejects.toThrow(FaaSError);
      });
    });

    describe('getSnapshot', () => {
      it('should retrieve snapshot by ID', async () => {
        mockFetch.mockResolvedValueOnce({
          ok: true,
          json: async () => mockSnapshot,
        } as any);

        const result = await client.getSnapshot('snap-123');

        expect(result).toEqual(mockSnapshot);
        expect(mockFetch).toHaveBeenCalledWith(
          `${mockBaseUrl}/snapshots/snap-123`,
          expect.any(Object)
        );
      });
    });

    describe('listSnapshots', () => {
      it('should list snapshots with filter', async () => {
        mockFetch.mockResolvedValueOnce({
          ok: true,
          json: async () => [mockSnapshot],
        } as any);

        const result = await client.listSnapshots({
          parent_id: 'snap-parent',
        });

        expect(result).toEqual([mockSnapshot]);
        expect(mockFetch).toHaveBeenCalledWith(
          expect.stringContaining('parent_id=snap-parent'),
          expect.any(Object)
        );
      });
    });

    describe('execOnSnapshot', () => {
      it('should execute command on snapshot', async () => {
        const mockResult = {
          snapshot: mockSnapshot,
          result: {
            exit_code: 0,
            stdout: 'Hello World',
            stderr: '',
            duration_ms: 100,
          },
        };

        mockFetch.mockResolvedValueOnce({
          ok: true,
          json: async () => mockResult,
        } as any);

        const result = await client.execOnSnapshot('snap-123', 'echo "Hello World"');

        expect(result).toEqual(mockResult);
        expect(mockFetch).toHaveBeenCalledWith(
          `${mockBaseUrl}/snapshots/snap-123/exec`,
          expect.objectContaining({
            method: 'POST',
            body: expect.stringContaining('echo'),
          })
        );
      });
    });
  });

  describe('Instance Operations', () => {
    const mockInstance: Instance = {
      id: 'inst-123',
      snapshot_id: 'snap-123',
      state: InstanceState.RUNNING,
      created_at: '2024-01-01T00:00:00Z',
      updated_at: '2024-01-01T00:01:00Z',
      endpoints: {
        ssh: 'ssh://inst-123.faas.io',
      },
    };

    describe('startInstance', () => {
      it('should start instance from snapshot', async () => {
        mockFetch.mockResolvedValueOnce({
          ok: true,
          json: async () => mockInstance,
        } as any);

        const result = await client.startInstance({
          snapshot_id: 'snap-123',
          ttl: 3600,
          auto_stop: true,
        });

        expect(result).toEqual(mockInstance);
        expect(mockFetch).toHaveBeenCalledWith(
          `${mockBaseUrl}/instances`,
          expect.objectContaining({
            method: 'POST',
            body: expect.stringContaining('snap-123'),
          })
        );
      });
    });

    describe('waitForInstance', () => {
      it('should wait for instance to reach target state', async () => {
        const pendingInstance = { ...mockInstance, state: InstanceState.PENDING };
        const runningInstance = { ...mockInstance, state: InstanceState.RUNNING };

        mockFetch
          .mockResolvedValueOnce({
            ok: true,
            json: async () => pendingInstance,
          } as any)
          .mockResolvedValueOnce({
            ok: true,
            json: async () => runningInstance,
          } as any);

        const result = await client.waitForInstance('inst-123', InstanceState.RUNNING, 5000);

        expect(result).toEqual(runningInstance);
        expect(mockFetch).toHaveBeenCalledTimes(2);
      });

      it('should timeout if state not reached', async () => {
        mockFetch.mockResolvedValue({
          ok: true,
          json: async () => ({ ...mockInstance, state: InstanceState.PENDING }),
        } as any);

        await expect(
          client.waitForInstance('inst-123', InstanceState.RUNNING, 100)
        ).rejects.toThrow(TimeoutError);
      });

      it('should throw if instance terminates', async () => {
        mockFetch.mockResolvedValueOnce({
          ok: true,
          json: async () => ({ ...mockInstance, state: InstanceState.TERMINATED }),
        } as any);

        await expect(
          client.waitForInstance('inst-123', InstanceState.RUNNING)
        ).rejects.toThrow('terminated unexpectedly');
      });
    });

    describe('execOnInstance', () => {
      it('should execute command on instance', async () => {
        const mockResult: ExecutionResult = {
          exit_code: 0,
          stdout: 'test output',
          stderr: '',
          duration_ms: 50,
        };

        mockFetch.mockResolvedValueOnce({
          ok: true,
          json: async () => mockResult,
        } as any);

        const result = await client.execOnInstance('inst-123', 'ls -la');

        expect(result).toEqual(mockResult);
        expect(mockFetch).toHaveBeenCalledWith(
          `${mockBaseUrl}/instances/inst-123/exec`,
          expect.objectContaining({
            method: 'POST',
            body: expect.stringContaining('ls -la'),
          })
        );
      });
    });

    describe('Instance State Management', () => {
      it('should stop instance', async () => {
        mockFetch.mockResolvedValueOnce({
          ok: true,
          json: async () => ({}),
        } as any);

        await client.stopInstance('inst-123');

        expect(mockFetch).toHaveBeenCalledWith(
          `${mockBaseUrl}/instances/inst-123/stop`,
          expect.objectContaining({ method: 'POST' })
        );
      });

      it('should pause instance', async () => {
        mockFetch.mockResolvedValueOnce({
          ok: true,
          json: async () => ({}),
        } as any);

        await client.pauseInstance('inst-123');

        expect(mockFetch).toHaveBeenCalledWith(
          `${mockBaseUrl}/instances/inst-123/pause`,
          expect.objectContaining({ method: 'POST' })
        );
      });

      it('should resume instance', async () => {
        mockFetch.mockResolvedValueOnce({
          ok: true,
          json: async () => ({}),
        } as any);

        await client.resumeInstance('inst-123');

        expect(mockFetch).toHaveBeenCalledWith(
          `${mockBaseUrl}/instances/inst-123/resume`,
          expect.objectContaining({ method: 'POST' })
        );
      });
    });
  });

  describe('Branch Operations', () => {
    const mockBranch = {
      id: 'branch-123',
      parent_id: 'branch-parent',
      name: 'feature-branch',
      divergence_point: 'snap-123',
      created_at: '2024-01-01T00:00:00Z',
    };

    describe('createBranch', () => {
      it('should create branch from snapshot', async () => {
        mockFetch.mockResolvedValueOnce({
          ok: true,
          json: async () => mockBranch,
        } as any);

        const result = await client.createBranch({
          snapshot_id: 'snap-123',
          name: 'feature-branch',
        });

        expect(result).toEqual(mockBranch);
        expect(mockFetch).toHaveBeenCalledWith(
          `${mockBaseUrl}/branches`,
          expect.objectContaining({
            method: 'POST',
            body: expect.stringContaining('feature-branch'),
          })
        );
      });
    });

    describe('mergeBranches', () => {
      it('should merge multiple branches', async () => {
        const mockSnapshot: Snapshot = {
          id: 'snap-merged',
          created_at: '2024-01-01T00:00:00Z',
          size: 2048,
          checksum: 'merged123',
        };

        mockFetch.mockResolvedValueOnce({
          ok: true,
          json: async () => mockSnapshot,
        } as any);

        const result = await client.mergeBranches(['branch-1', 'branch-2']);

        expect(result).toEqual(mockSnapshot);
        expect(mockFetch).toHaveBeenCalledWith(
          `${mockBaseUrl}/branches/merge`,
          expect.objectContaining({
            method: 'POST',
            body: expect.stringContaining('branch-1'),
          })
        );
      });
    });
  });

  describe('Network Operations', () => {
    describe('exposeHttpService', () => {
      it('should expose HTTP service', async () => {
        const mockService = {
          name: 'web',
          port: 8080,
          url: 'https://web-inst-123.faas.io',
          exposed_at: '2024-01-01T00:00:00Z',
        };

        mockFetch.mockResolvedValueOnce({
          ok: true,
          json: async () => mockService,
        } as any);

        const result = await client.exposeHttpService('inst-123', 'web', 8080);

        expect(result).toEqual(mockService);
        expect(mockFetch).toHaveBeenCalledWith(
          `${mockBaseUrl}/instances/inst-123/services/http`,
          expect.objectContaining({
            method: 'POST',
            body: expect.stringContaining('8080'),
          })
        );
      });
    });

    describe('createPortForward', () => {
      it('should create port forward', async () => {
        const mockForward = {
          local_port: 3000,
          remote_port: 8080,
          active: true,
        };

        mockFetch.mockResolvedValueOnce({
          ok: true,
          json: async () => mockForward,
        } as any);

        const result = await client.createPortForward('inst-123', 3000, 8080);

        expect(result).toEqual(mockForward);
        expect(mockFetch).toHaveBeenCalledWith(
          `${mockBaseUrl}/instances/inst-123/port-forward`,
          expect.objectContaining({
            method: 'POST',
            body: expect.stringContaining('3000'),
          })
        );
      });
    });
  });

  describe('Error Handling', () => {
    it('should retry on 5xx errors', async () => {
      mockFetch
        .mockRejectedValueOnce(new Error('Network error'))
        .mockResolvedValueOnce({
          ok: true,
          json: async () => ({ id: 'success' }),
        } as any);

      const result = await client.getSnapshot('snap-123');

      expect(result).toEqual({ id: 'success' });
      expect(mockFetch).toHaveBeenCalledTimes(2);
    });

    it('should not retry on 4xx errors', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: false,
        status: 404,
        json: async () => ({ message: 'Not found' }),
      } as any);

      await expect(client.getSnapshot('snap-123')).rejects.toThrow(FaaSError);
      expect(mockFetch).toHaveBeenCalledTimes(1);
    });

    it('should handle timeout errors', async () => {
      const slowResponse = new Promise((resolve) => {
        setTimeout(() => resolve({ ok: true, json: async () => ({}) }), 10000);
      });

      mockFetch.mockReturnValueOnce(slowResponse as any);

      const fastClient = new FaaSClient({
        apiKey: mockApiKey,
        timeout: 100,
      });

      await expect(fastClient.getSnapshot('snap-123')).rejects.toThrow(TimeoutError);
    });
  });

  describe('Parallel Execution', () => {
    it('should execute tasks in parallel', async () => {
      const mockSnapshots = [
        { id: 'snap-1', created_at: '2024-01-01T00:00:00Z', size: 100, checksum: '1' },
        { id: 'snap-2', created_at: '2024-01-01T00:00:00Z', size: 200, checksum: '2' },
      ];

      const mockInstances = [
        { id: 'inst-1', snapshot_id: 'snap-1', state: InstanceState.RUNNING },
        { id: 'inst-2', snapshot_id: 'snap-2', state: InstanceState.RUNNING },
      ];

      let snapshotIndex = 0;
      let instanceIndex = 0;

      mockFetch.mockImplementation((url: any) => {
        if (url.includes('/snapshots') && !url.includes('/snapshots/')) {
          return Promise.resolve({
            ok: true,
            json: async () => mockSnapshots[snapshotIndex++],
          } as any);
        }
        if (url.includes('/instances') && !url.includes('/instances/')) {
          return Promise.resolve({
            ok: true,
            json: async () => mockInstances[instanceIndex++],
          } as any);
        }
        if (url.includes('/instances/inst-')) {
          return Promise.resolve({
            ok: true,
            json: async () => ({}),
          } as any);
        }
        return Promise.resolve({
          ok: true,
          json: async () => ({}),
        } as any);
      });

      const items = ['item1', 'item2'];
      const results = await client.parallelExec(items, async (item, instance) => {
        return `${item}-${instance.id}`;
      });

      expect(results).toEqual(['item1-inst-1', 'item2-inst-2']);
    });
  });
});