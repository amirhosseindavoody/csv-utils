const std = @import("std");

pub const Parsed = struct {
    file_path: []const u8,
};

pub fn requireFileArg(iter: *std.process.ArgIterator) !Parsed {
    const file_path = iter.next() orelse return error.MissingFilePath;
    return Parsed{ .file_path = file_path };
}
