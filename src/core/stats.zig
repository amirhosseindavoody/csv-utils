const std = @import("std");

pub const ColStats = struct {
    rows: usize = 0,
    nulls: usize = 0,
    non_nulls: usize = 0,
    max_width: usize = 0,
};

pub const StatsAgg = struct {
    allocator: std.mem.Allocator,
    cols: []ColStats,

    pub fn init(allocator: std.mem.Allocator, col_count: usize) !StatsAgg {
        const cols = try allocator.alloc(ColStats, col_count);
        for (cols) |*c| c.* = .{};
        return .{
            .allocator = allocator,
            .cols = cols,
        };
    }

    pub fn deinit(self: *StatsAgg) void {
        self.allocator.free(self.cols);
    }

    pub fn observe(self: *StatsAgg, fields: []const []const u8) !void {
        const limit = @min(fields.len, self.cols.len);
        for (fields[0..limit], 0..) |value, i| {
            self.cols[i].rows += 1;
            if (value.len == 0) {
                self.cols[i].nulls += 1;
            } else {
                self.cols[i].non_nulls += 1;
                self.cols[i].max_width = @max(self.cols[i].max_width, value.len);
            }
        }
    }

    pub fn print(self: *StatsAgg, headers: []const []const u8) !void {
        for (self.cols, 0..) |c, i| {
            const name = if (i < headers.len) headers[i] else "unknown";
            std.debug.print(
                "{s}: rows={d} nulls={d} non_nulls={d} max_width={d}\n",
                .{ name, c.rows, c.nulls, c.non_nulls, c.max_width },
            );
        }
    }
};
