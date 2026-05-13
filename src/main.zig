const std = @import("std");
const cli = @import("cli/commands.zig");
const tui = @import("tui/app.zig");

pub fn main(init: std.process.Init) !void {
    const allocator = init.gpa;

    var args = try std.process.Args.Iterator.initAllocator(init.minimal.args, allocator);
    defer args.deinit();

    _ = args.next();
    const mode_or_cmd_z = args.next() orelse {
        try cli.printHelp();
        return;
    };
    const mode_or_cmd: []const u8 = mode_or_cmd_z;

    if (std.mem.eql(u8, mode_or_cmd, "tui")) {
        const maybe_path_z = args.next();
        const maybe_path: ?[]const u8 = if (maybe_path_z) |p| p else null;
        try tui.run(init.io, init.environ_map, maybe_path);
        return;
    }

    try cli.runCommand(init.io, allocator, mode_or_cmd, &args);
}
