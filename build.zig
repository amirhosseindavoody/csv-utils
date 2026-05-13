const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});
    const vaxis_dep = b.dependency("vaxis", .{
        .target = target,
        .optimize = optimize,
    });

    const exe = b.addExecutable(.{
        .name = "csv-utils",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/main.zig"),
            .target = target,
            .optimize = optimize,
        }),
    });
    exe.root_module.addImport("vaxis", vaxis_dep.module("vaxis"));
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
    tests.root_module.addImport("vaxis", vaxis_dep.module("vaxis"));
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

    const debug_preview_exe = b.addExecutable(.{
        .name = "debug-preview-dump",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/debug_preview_dump.zig"),
            .target = target,
            .optimize = optimize,
        }),
    });
    b.installArtifact(debug_preview_exe);

    const run_debug_preview = b.addRunArtifact(debug_preview_exe);
    if (b.args) |da| run_debug_preview.addArgs(da);
    const debug_preview_step = b.step("debug-preview", "Dump first parsed preview header/row");
    debug_preview_step.dependOn(&run_debug_preview.step);
}
