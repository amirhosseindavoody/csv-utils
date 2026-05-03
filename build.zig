const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    const exe = b.addExecutable(.{
        .name = "csv-utils",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/main.zig"),
            .target = target,
            .optimize = optimize,
            .link_libc = true,
        }),
    });
    exe.root_module.addIncludePath(b.path(".pixi/envs/default/include"));
    exe.root_module.addLibraryPath(b.path(".pixi/envs/default/lib"));
    exe.root_module.addRPath(b.path(".pixi/envs/default/lib"));
    // `libncursesw.so` in conda is a linker script (ASCII); Zig expects ELF. Link the versioned .so.* files.
    exe.root_module.addObjectFile(b.path(".pixi/envs/default/lib/libncursesw.so.6.6"));
    exe.root_module.addObjectFile(b.path(".pixi/envs/default/lib/libtinfow.so.6.6"));
    b.installArtifact(exe);

    const run_cmd = b.addRunArtifact(exe);
    if (b.args) |args| {
        run_cmd.addArgs(args);
    }

    const run_step = b.step("run", "Run the csv-utils binary");
    run_step.dependOn(&run_cmd.step);

    const tests = b.addTest(.{
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/main.zig"),
            .target = target,
            .optimize = optimize,
        }),
    });
    const run_tests = b.addRunArtifact(tests);
    const test_step = b.step("test", "Run unit tests");
    test_step.dependOn(&run_tests.step);

    const bench_exe = b.addExecutable(.{
        .name = "bench-csv-parse",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/bench_csv_parse_main.zig"),
            .target = target,
            .optimize = .ReleaseFast,
        }),
    });
    b.installArtifact(bench_exe);

    const run_bench = b.addRunArtifact(bench_exe);
    if (b.args) |ba| run_bench.addArgs(ba);
    const bench_step = b.step("bench-parse", "Benchmark CSV preview load (same as TUI initial load)");
    bench_step.dependOn(&run_bench.step);
}
