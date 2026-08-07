# Rust toolchain for cross-compiling to the aarch64-unknown-none-softfloat
# bare-metal target. Runs the host `rustc`/`rustup` toolchain (with the
# `aarch64-unknown-none-softfloat` target component installed) as a
# cross-compiler; execution still happens on the host execution platform,
# only the target *configuration* changes.

load("@prelude//rust:rust_toolchain.bzl", "PanicRuntime", "RustToolchainInfo")

def _cross_rust_toolchain_impl(ctx):
    triple = ctx.attrs.rustc_target_triple

    # Only the bare-metal target needs these: a `None`/host triple here means
    # this toolchain instance is compiling a proc-macro or build-script
    # (always host-configured, see buck/toolchains/BUCK), which must build as
    # an ordinary host binary/dylib with unwinding enabled.
    bare_metal = triple != None and "none" in triple

    if bare_metal:
        rustc_flags = [
            # Tell rustc our linker is a plain, standalone gnu-ld-compatible
            # binary (real `ld.lld`), not the rustc-bundled multiplexed
            # `rust-lld` - otherwise rustc prepends `-flavor <name>`, a flag
            # only that multiplexer binary understands.
            "-C", "linker-flavor=ld",
            "-C", "link-arg=--no-eh-frame-hdr",
            "-C", "force-unwind-tables=no",
            "-C", "relocation-model=static",
            "-C", "code-model=large",
            "-C", "link-arg=-z",
            "-C", "link-arg=max-page-size=4096",
        ]
        if ctx.attrs.lto:
            # Emit LLVM bitcode object files so the final cxx link step can
            # run distributed ThinLTO across the produced rlib/archive.
            rustc_flags += ["-C", "linker-plugin-lto"]
        panic_runtime = PanicRuntime("abort")
    else:
        rustc_flags = []
        panic_runtime = PanicRuntime("unwind")

    return [
        DefaultInfo(),
        RustToolchainInfo(
            allow_lints = [],
            clippy_driver = RunInfo(args = ["clippy-driver"]),
            clippy_toml = None,
            compiler = RunInfo(args = ["rustc"]),
            default_edition = ctx.attrs.default_edition,
            panic_runtime = panic_runtime,
            deny_lints = [],
            doctests = False,
            nightly_features = True,
            report_unused_deps = False,
            rustc_binary_flags = [],
            rustc_flags = rustc_flags,
            rustc_target_triple = triple,
            rustc_test_flags = [],
            rustdoc = RunInfo(args = ["rustdoc"]),
            rustdoc_flags = [],
            warn_lints = [],
        ),
    ]

cross_rust_toolchain = rule(
    impl = _cross_rust_toolchain_impl,
    attrs = {
        "default_edition": attrs.option(attrs.string(), default = None),
        "lto": attrs.bool(default = False),
        "rustc_target_triple": attrs.option(attrs.string(), default = None),
    },
    is_toolchain_rule = True,
)
