const std = @import("std");

pub const Parsed = struct {
    file_path: []const u8,
};

pub fn requireFileArg(iter: *std.process.Args.Iterator) !Parsed {
    const file_path_z = iter.next() orelse return error.MissingFilePath;
    const file_path: []const u8 = file_path_z;
    return Parsed{ .file_path = file_path };
}
