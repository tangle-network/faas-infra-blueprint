fn main() {
    // Build SP1 guest program examples
    sp1_build::build_program_with_args(
        "sp1-guest-examples/fibonacci-example",
        sp1_build::BuildArgs::default(),
    );

    sp1_build::build_program_with_args(
        "sp1-guest-examples/hash-preimage-example",
        sp1_build::BuildArgs::default(),
    );
}
