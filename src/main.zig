const std = @import("std");
const cli = @import("cli/commands.zig");
const tui = @import("tui/app.zig");

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    var args = try std.process.argsWithAllocator(allocator);
    defer args.deinit();

    _ = args.next();
    const mode_or_cmd = args.next() orelse {
        try cli.printHelp();
        return;
    };

    if (std.mem.eql(u8, mode_or_cmd, "tui")) {
        try tui.run(args.next());
        return;
    }

    try cli.runCommand(allocator, mode_or_cmd, &args);
}
