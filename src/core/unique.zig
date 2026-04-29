const std = @import("std");
const reader_mod = @import("csv_reader.zig");
const schema = @import("schema.zig");

pub fn printUniqueForColumns(
    allocator: std.mem.Allocator,
    csv: *reader_mod.CsvReader,
    headers: []const []const u8,
    indexes: []const usize,
    cap: usize,
) !void {
    var map = std.StringHashMap(void).init(allocator);
    defer {
        var it = map.keyIterator();
        while (it.next()) |k| allocator.free(k.*);
        map.deinit();
    }

    while (try csv.nextLine()) |line| {
        defer allocator.free(line);
        if (map.count() >= cap) break;
        var fields = try schema.splitRow(allocator, line);
        defer fields.deinit();

        var key_builder = std.ArrayList(u8){};
        defer key_builder.deinit(allocator);
        for (indexes, 0..) |idx, i| {
            if (idx >= fields.items.len) continue;
            if (i != 0) try key_builder.append(allocator, 0x1f);
            try key_builder.appendSlice(allocator, fields.items[idx]);
        }
        const key = try key_builder.toOwnedSlice(allocator);

        if (map.contains(key)) {
            allocator.free(key);
            continue;
        }
        const copy = try allocator.dupe(u8, key);
        allocator.free(key);
        try map.put(copy, {});

        var it_parts = std.mem.splitScalar(u8, copy, 0x1f);
        std.debug.print("{{", .{});
        var col_i: usize = 0;
        while (it_parts.next()) |val| : (col_i += 1) {
            if (col_i != 0) std.debug.print(", ", .{});
            const idx = indexes[col_i];
            const name = if (idx < headers.len) headers[idx] else "unknown";
            std.debug.print("\"{s}\": \"{s}\"", .{ name, val });
        }
        std.debug.print("}}\n", .{});
    }
}
