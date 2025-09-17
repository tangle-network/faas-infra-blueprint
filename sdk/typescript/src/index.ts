/**
 * FaaS SDK for TypeScript
 */

export * from './client';
export * from './tangle-integration';

// Re-export main client as default
import { FaaSClient } from './client';
export default FaaSClient;