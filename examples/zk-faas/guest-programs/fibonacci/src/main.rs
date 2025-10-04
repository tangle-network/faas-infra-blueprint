//! Fibonacci Guest Program for SP1 zkVM
//! 
//! Computes Fibonacci numbers and commits results to public outputs.
//! This is the code that runs inside the zkVM and gets proven.

#![no_main]
sp1_zkvm::entrypoint!(main);

pub fn main() {
    // Read input (n) from stdin
    let n = sp1_zkvm::io::read::<u32>();
    
    // Commit input to public outputs
    sp1_zkvm::io::commit(&n);

    // Compute Fibonacci
    let mut a: u32 = 0;
    let mut b: u32 = 1;
    
    for _ in 0..n {
        let mut c = a.wrapping_add(b);
        c %= 7919; // Modulus to prevent overflow
        a = b;
        b = c;
    }

    // Commit results to public outputs
    sp1_zkvm::io::commit(&a);
    sp1_zkvm::io::commit(&b);
    
    println!("cycle-tracker-start: fibonacci-computation");
    // Additional computation to show cycle tracking
    let _sum = a.wrapping_add(b);
    println!("cycle-tracker-end: fibonacci-computation");
}
