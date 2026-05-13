const std = @import("std");
const reader_mod = @import("csv_reader.zig");
const schema = @import("schema.zig");
const stats_mod = @import("stats.zig");
const json_view = @import("json_view.zig");
const unique_mod = @import("unique.zig");
const predicate = @import("predicate.zig");

pub fn printBasicStats(io: std.Io, allocator: std.mem.Allocator, file_path: []const u8) !void {
    var file = try std.Io.Dir.cwd().openFile(io, file_path, .{});
    defer file.close(io);

    var csv = reader_mod.CsvReader.init(io, allocator, file);
    defer csv.deinit();

    const header_line = (try csv.nextLine()) orelse return error.EmptyCsv;
    defer allocator.free(header_line);
    var headers = try schema.splitRow(allocator, header_line);
    defer headers.deinit();

    var agg = try stats_mod.StatsAgg.init(allocator, headers.items.len);
    defer agg.deinit();

    while (try csv.nextLine()) |line| {
        defer allocator.free(line);
        var fields = try schema.splitRow(allocator, line);
        defer fields.deinit();
        try agg.observe(fields.items);
    }

    try agg.print(headers.items);
}

pub fn printUniqueValues(
    io: std.Io,
    allocator: std.mem.Allocator,
    file_path: []const u8,
    columns_expr: []const u8,
    cap: usize,
) !void {
    var file = try std.Io.Dir.cwd().openFile(io, file_path, .{});
    defer file.close(io);

    var csv = reader_mod.CsvReader.init(io, allocator, file);
    defer csv.deinit();

    const header_line = (try csv.nextLine()) orelse return error.EmptyCsv;
    defer allocator.free(header_line);
    var headers = try schema.splitRow(allocator, header_line);
    defer headers.deinit();

    var parts = std.mem.splitScalar(u8, columns_expr, ',');
    var indexes: std.ArrayList(usize) = .empty;
    defer indexes.deinit(allocator);
    while (parts.next()) |p| {
        const col = std.mem.trim(u8, p, " ");
        if (col.len == 0) continue;
        const idx = schema.indexOf(headers.items, col) orelse return error.ColumnNotFound;
        try indexes.append(allocator, idx);
    }
    if (indexes.items.len == 0) return error.MissingColumnName;
    try unique_mod.printUniqueForColumns(allocator, &csv, headers.items, indexes.items, cap);
}

pub fn printRowsAsJson(io: std.Io, allocator: std.mem.Allocator, file_path: []const u8, limit: usize) !void {
    var file = try std.Io.Dir.cwd().openFile(io, file_path, .{});
    defer file.close(io);

    var csv = reader_mod.CsvReader.init(io, allocator, file);
    defer csv.deinit();

    const header_line = (try csv.nextLine()) orelse return error.EmptyCsv;
    defer allocator.free(header_line);
    var headers = try schema.splitRow(allocator, header_line);
    defer headers.deinit();

    var emitted: usize = 0;
    while (emitted < limit) : (emitted += 1) {
        const line = (try csv.nextLine()) orelse break;
        defer allocator.free(line);
        var fields = try schema.splitRow(allocator, line);
        defer fields.deinit();
        try json_view.printRow(headers.items, fields.items);
    }
}

pub fn printFilteredRows(
    io: std.Io,
    allocator: std.mem.Allocator,
    file_path: []const u8,
    filter_expr: []const u8,
    limit: usize,
) !void {
    var file = try std.Io.Dir.cwd().openFile(io, file_path, .{});
    defer file.close(io);

    var csv = reader_mod.CsvReader.init(io, allocator, file);
    defer csv.deinit();

    const header_line = (try csv.nextLine()) orelse return error.EmptyCsv;
    defer allocator.free(header_line);
    var headers = try schema.splitRow(allocator, header_line);
    defer headers.deinit();

    const conditions = try predicate.parseConditions(allocator, filter_expr);
    defer allocator.free(conditions);
    if (conditions.len == 0) return error.InvalidFilterExpression;

    const resolved = try allocator.alloc(predicate.ResolvedCondition, conditions.len);
    defer allocator.free(resolved);
    for (conditions, 0..) |cond, i| {
        const idx = schema.indexOf(headers.items, cond.column) orelse return error.ColumnNotFound;
        resolved[i] = .{ .index = idx, .op = cond.op, .value = cond.value };
    }

    var emitted: usize = 0;
    while (emitted < limit) {
        const line = (try csv.nextLine()) orelse break;
        defer allocator.free(line);
        var fields = try schema.splitRow(allocator, line);
        defer fields.deinit();
        if (predicate.rowMatchesAll(fields.items, resolved)) {
            try json_view.printRow(headers.items, fields.items);
            emitted += 1;
        }
    }
}
