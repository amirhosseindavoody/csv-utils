const std = @import("std");

pub fn printRow(headers: []const []const u8, fields: []const []const u8) !void {
    std.debug.print("{{", .{});

    const limit = @min(headers.len, fields.len);
    for (0..limit) |i| {
        if (i != 0) std.debug.print(", ", .{});
        std.debug.print("\"{s}\": \"{s}\"", .{ headers[i], fields[i] });
    }

    std.debug.print("}}\n", .{});
}
