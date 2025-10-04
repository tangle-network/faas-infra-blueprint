fn main() {
    // Build SP1 guest programs
    sp1_build::build_program_with_args(
        "guest-programs/fibonacci",
        sp1_build::BuildArgs::default(),
    );

    sp1_build::build_program_with_args(
        "guest-programs/hash-preimage",
        sp1_build::BuildArgs::default(),
    );
}
