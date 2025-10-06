//! Hash Preimage Guest Program for SP1 zkVM
//!
//! Proves knowledge of a preimage for a given hash without revealing the preimage.

#![no_main]
sp1_zkvm::entrypoint!(main);

use sha2::{Digest, Sha256};

pub fn main() {
    // Read the secret preimage (private input)
    let preimage = sp1_zkvm::io::read::<Vec<u8>>();

    // Read the expected hash (public input)
    let expected_hash = sp1_zkvm::io::read::<[u8; 32]>();

    // Commit the expected hash to public outputs
    sp1_zkvm::io::commit(&expected_hash);

    // Compute hash of preimage
    let mut hasher = Sha256::new();
    hasher.update(&preimage);
    let computed_hash: [u8; 32] = hasher.finalize().into();

    // Verify hash matches
    assert_eq!(
        computed_hash, expected_hash,
        "Preimage does not match expected hash"
    );

    // Commit result (true if hash matches)
    sp1_zkvm::io::commit(&true);

    // Commit computed hash for verification
    sp1_zkvm::io::commit(&computed_hash);
}
