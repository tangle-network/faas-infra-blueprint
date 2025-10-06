//! Fibonacci Guest Program for SP1 zkVM

#![no_main]
sp1_zkvm::entrypoint!(main);

pub fn main() {
    // Read input
    let n = sp1_zkvm::io::read::<u32>();

    // Commit public input
    sp1_zkvm::io::commit(&n);

    // Compute Fibonacci
    let mut a: u64 = 0;
    let mut b: u64 = 1;

    for _ in 0..n {
        let mut c = a + b;
        c %= 7919; // Modulus to prevent overflow
        a = b;
        b = c;
    }

    // Commit result
    sp1_zkvm::io::commit(&a);
    sp1_zkvm::io::commit(&b);
}
