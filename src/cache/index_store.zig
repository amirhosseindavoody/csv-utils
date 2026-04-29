const std = @import("std");

pub const FileFingerprint = struct {
    size: u64,
    mtime: i128,
};

pub fn fingerprint(file: std.fs.File) !FileFingerprint {
    const stat = try file.stat();
    return .{
        .size = stat.size,
        .mtime = stat.mtime,
    };
}
