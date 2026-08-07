# cxx toolchain used to link the aarch64-unknown-none-softfloat rust_binary.
#
# rust_binary's final link step always goes through the cxx linking
# machinery, so even though this project has no C/C++ sources we still need a
# cxx_toolchain: one configured to link bare-metal aarch64 ELF binaries with
# rust-lld (bundled with rustc), and with distributed ThinLTO turned on.

load(
    "@prelude//cxx:cxx_toolchain_types.bzl",
    "BinaryUtilitiesInfo",
    "CCompilerInfo",
    "CxxCompilerInfo",
    "CxxInternalTools",
    "CxxToolchainInfo",
    "DepTrackingMode",
    "LinkerInfo",
    "LinkerType",
    "PicBehavior",
    "RuntimeDependencyHandling",
    "ShlibInterfacesMode",
)
load("@prelude//cxx:headers.bzl", "HeaderMode")
load("@prelude//linking:link_info.bzl", "LinkOrdering", "LinkStyle")
load("@prelude//linking:lto.bzl", "LtoMode")

def _run_info(binary):
    return RunInfo(args = [binary])

def _cross_cxx_toolchain_impl(ctx):
    lto_mode = LtoMode("thin") if ctx.attrs.lto else LtoMode("none")
    linker_type = LinkerType(ctx.attrs.linker_type)

    return [
        DefaultInfo(),
        CxxToolchainInfo(
            internal_tools = ctx.attrs.internal_tools[CxxInternalTools],
            linker_info = LinkerInfo(
                linker = _run_info(ctx.attrs.linker),
                linker_flags = ctx.attrs.link_flags,
                post_linker_flags = [],
                archiver = _run_info(ctx.attrs.archiver),
                archiver_type = "gnu",
                archiver_supports_argfiles = True,
                generate_linker_maps = False,
                lto_mode = lto_mode,
                supports_distributed_thinlto = ctx.attrs.lto,
                type = linker_type,
                link_binaries_locally = True,
                link_libraries_locally = True,
                archive_objects_locally = True,
                use_archiver_flags = True,
                static_dep_runtime_ld_flags = [],
                static_pic_dep_runtime_ld_flags = [],
                shared_dep_runtime_ld_flags = [],
                independent_shlib_interface_linker_flags = [],
                shlib_interfaces = ShlibInterfacesMode("disabled"),
                link_style = LinkStyle("static"),
                link_weight = 1,
                binary_extension = "elf",
                object_file_extension = "o",
                shared_library_name_default_prefix = "lib",
                shared_library_name_format = ctx.attrs.shared_library_name_format,
                shared_library_versioned_name_format = ctx.attrs.shared_library_name_format + ".{}",
                static_library_extension = "a",
                force_full_hybrid_if_capable = False,
                is_pdb_generated = False,
                link_ordering = None,
            ),
            bolt_enabled = False,
            binary_utilities_info = BinaryUtilitiesInfo(
                nm = _run_info(ctx.attrs.nm),
                objcopy = _run_info(ctx.attrs.objcopy),
                objdump = _run_info(ctx.attrs.objdump),
                ranlib = _run_info(ctx.attrs.ranlib),
                strip = _run_info(ctx.attrs.strip),
                dwp = None,
                bolt_msdk = None,
            ),
            cxx_compiler_info = CxxCompilerInfo(
                compiler = _run_info(ctx.attrs.cxx_compiler),
                preprocessor_flags = [],
                compiler_flags = [],
                compiler_type = "clang",
                supports_two_phase_compilation = False,
                supports_content_based_paths = False,
            ),
            c_compiler_info = CCompilerInfo(
                compiler = _run_info(ctx.attrs.compiler),
                preprocessor_flags = [],
                compiler_flags = [],
                compiler_type = "clang",
                supports_content_based_paths = False,
            ),
            as_compiler_info = CCompilerInfo(
                compiler = _run_info(ctx.attrs.compiler),
                compiler_type = "clang",
                supports_content_based_paths = False,
            ),
            asm_compiler_info = CCompilerInfo(
                compiler = _run_info(ctx.attrs.compiler),
                compiler_type = "clang",
            ),
            header_mode = HeaderMode("symlink_tree_only"),
            cpp_dep_tracking_mode = DepTrackingMode("none"),
            pic_behavior = PicBehavior("always_enabled") if linker_type == LinkerType("darwin") else PicBehavior("supported"),
            llvm_link = _run_info(ctx.attrs.llvm_link),
            use_dep_files = False,
            runtime_dependency_handling = RuntimeDependencyHandling("no_symlink"),
        ),
    ]

cross_cxx_toolchain = rule(
    impl = _cross_cxx_toolchain_impl,
    attrs = {
        "archiver": attrs.string(default = "llvm-ar"),
        "compiler": attrs.string(default = "clang"),
        "cxx_compiler": attrs.string(default = "clang++"),
        "internal_tools": attrs.default_only(attrs.exec_dep(providers = [CxxInternalTools], default = "prelude//cxx/tools:internal_tools")),
        "link_flags": attrs.list(attrs.arg(), default = []),
        "linker": attrs.string(default = "rust-lld"),
        "linker_type": attrs.string(default = "gnu"),
        "llvm_link": attrs.string(default = "llvm-link"),
        "lto": attrs.bool(default = False),
        "nm": attrs.string(default = "llvm-nm"),
        "objcopy": attrs.string(default = "llvm-objcopy"),
        "shared_library_name_format": attrs.string(default = "{}.so"),
        "objdump": attrs.string(default = "llvm-objdump"),
        "ranlib": attrs.string(default = "llvm-ranlib"),
        "strip": attrs.string(default = "llvm-strip"),
    },
    is_toolchain_rule = True,
)
