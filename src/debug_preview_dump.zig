const std = @import("std");
const pv = @import("core/preview.zig");

pub fn main(init: std.process.Init) !void {
    var args = try std.process.Args.Iterator.initAllocator(init.minimal.args, init.gpa);
    defer args.deinit();
    _ = args.next();
    const path_z = args.next() orelse "test-data/generated/test_10000x1000.csv";
    const path: []const u8 = path_z;

    var data = try pv.loadPreviewHeaderAndInitialRows(init.io, std.heap.page_allocator, path, 1);
    defer data.deinit();

    std.debug.print("headers={d} rows={d}\n", .{ data.headers.len, data.rows.items.len });
    const show_cols = @min(data.headers.len, 8);
    for (0..show_cols) |i| {
        std.debug.print("h[{d}]={s}\n", .{ i, data.headers[i] });
    }
    if (data.rows.items.len > 0) {
        std.debug.print("row0={s}\n", .{data.rows.items[0]});
        var fields = try @import("core/schema.zig").splitRow(std.heap.page_allocator, data.rows.items[0]);
        defer fields.deinit();
        const show_fields = @min(fields.items.len, 8);
        for (0..show_fields) |i| {
            std.debug.print("f[{d}]={s}\n", .{ i, fields.items[i] });
        }
    }
}
