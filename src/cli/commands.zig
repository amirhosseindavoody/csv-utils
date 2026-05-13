const std = @import("std");
const parse_args = @import("args.zig");
const engine = @import("../core/engine.zig");

pub fn printHelp() !void {
    std.debug.print(
        \\csv-utils: high-performance CSV CLI + TUI
        \\
        \\Usage:
        \\  csv-utils stats <file.csv>
        \\  csv-utils unique <file.csv> <col1[,col2,...]> [limit]
        \\  csv-utils json <file.csv> [limit]
        \\  csv-utils filter <file.csv> <expr> [limit]
        \\    operators: =, !=, >, <, contains, in
        \\    examples:
        \\      city=Tehran,active=true
        \\      age>30
        \\      name contains Ali
        \\      city in Tehran|Paris
        \\  csv-utils tui [file.csv]
        \\
    , .{});
}

pub fn runCommand(
    io: std.Io,
    allocator: std.mem.Allocator,
    command: []const u8,
    args: *std.process.Args.Iterator,
) !void {
    if (std.mem.eql(u8, command, "stats")) {
        const parsed = try parse_args.requireFileArg(args);
        try engine.printBasicStats(io, allocator, parsed.file_path);
        return;
    }

    if (std.mem.eql(u8, command, "unique")) {
        const parsed = try parse_args.requireFileArg(args);
        const cols = args.next() orelse return error.MissingColumnName;
        const limit_text = args.next();
        const limit: usize = if (limit_text) |txt|
            try std.fmt.parseInt(usize, txt, 10)
        else
            50;
        try engine.printUniqueValues(io, allocator, parsed.file_path, cols, limit);
        return;
    }

    if (std.mem.eql(u8, command, "json")) {
        const parsed = try parse_args.requireFileArg(args);
        const limit_text = args.next();
        const limit: usize = if (limit_text) |txt|
            try std.fmt.parseInt(usize, txt, 10)
        else
            20;
        try engine.printRowsAsJson(io, allocator, parsed.file_path, limit);
        return;
    }

    if (std.mem.eql(u8, command, "filter")) {
        const parsed = try parse_args.requireFileArg(args);
        const expr = args.next() orelse return error.MissingFilterExpression;
        const limit_text = args.next();
        const limit: usize = if (limit_text) |txt|
            try std.fmt.parseInt(usize, txt, 10)
        else
            50;
        try engine.printFilteredRows(io, allocator, parsed.file_path, expr, limit);
        return;
    }

    try printHelp();
}
