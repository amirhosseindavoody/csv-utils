const std = @import("std");

pub const RowFields = struct {
    allocator: std.mem.Allocator,
    storage: []u8,
    items: []const []const u8,

    pub fn deinit(self: RowFields) void {
        self.allocator.free(self.items);
        self.allocator.free(self.storage);
    }
};

pub fn splitRow(allocator: std.mem.Allocator, line: []const u8) !RowFields {
    const trimmed = std.mem.trimRight(u8, line, "\r");
    const storage = try allocator.dupe(u8, trimmed);
    var fields = std.ArrayList([]const u8){};
    defer fields.deinit(allocator);

    var in_quotes = false;
    var field_start: usize = 0;
    var write_idx: usize = 0;
    var i: usize = 0;
    while (i < storage.len) : (i += 1) {
        const ch = storage[i];
        if (ch == '"') {
            if (in_quotes and i + 1 < storage.len and storage[i + 1] == '"') {
                storage[write_idx] = '"';
                write_idx += 1;
                i += 1;
                continue;
            }
            in_quotes = !in_quotes;
            continue;
        }

        if (ch == ',' and !in_quotes) {
            try fields.append(allocator, storage[field_start..write_idx]);
            field_start = write_idx + 1;
            storage[write_idx] = ',';
            write_idx += 1;
            continue;
        }

        storage[write_idx] = ch;
        write_idx += 1;
    }

    try fields.append(allocator, storage[field_start..write_idx]);
    const out = try fields.toOwnedSlice(allocator);
    return .{ .allocator = allocator, .storage = storage, .items = out };
}

pub fn indexOf(headers: []const []const u8, name: []const u8) ?usize {
    for (headers, 0..) |h, idx| {
        if (std.mem.eql(u8, h, name)) return idx;
    }
    return null;
}
