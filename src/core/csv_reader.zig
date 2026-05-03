const std = @import("std");

pub const CsvReader = struct {
    allocator: std.mem.Allocator,
    io_buf: [1024 * 1024]u8,
    file_reader: std.fs.File.Reader,

    pub fn init(allocator: std.mem.Allocator, file: std.fs.File) CsvReader {
        var self: CsvReader = undefined;
        self.allocator = allocator;
        self.io_buf = undefined;
        self.file_reader = file.reader(&self.io_buf);
        return self;
    }

    pub fn deinit(self: *CsvReader) void {
        _ = self;
    }

    pub fn nextLine(self: *CsvReader) !?[]const u8 {
        const slice = (try std.Io.Reader.takeDelimiter(&self.file_reader.interface, '\n')) orelse
            return null;
        return try self.allocator.dupe(u8, slice);
    }
};
