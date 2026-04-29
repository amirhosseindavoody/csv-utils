const std = @import("std");

pub const CsvReader = struct {
    allocator: std.mem.Allocator,
    reader: std.fs.File.DeprecatedReader,

    pub fn init(allocator: std.mem.Allocator, file: std.fs.File) CsvReader {
        return .{
            .allocator = allocator,
            .reader = file.deprecatedReader(),
        };
    }

    pub fn deinit(self: *CsvReader) void {
        _ = self;
    }

    pub fn nextLine(self: *CsvReader) !?[]const u8 {
        const line = try self.reader.readUntilDelimiterOrEofAlloc(
            self.allocator,
            '\n',
            1024 * 1024,
        );
        return line;
    }
};
