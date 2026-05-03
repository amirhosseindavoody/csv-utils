//! CSV preview: header row plus body lines stored as raw UTF-8 (cells parsed when rendered).
//! TUI loads a sync chunk (`loadPreviewHeaderAndInitialRows`) then `streamAppendBodyLinesAfterSkip` on a thread.
//! Bench uses `loadPreviewLimited` (sync, `scan_done` true).

const std = @import("std");
const schema = @import("schema.zig");

pub const PreviewData = struct {
    mutex: std.Thread.Mutex = .{},
    allocator: std.mem.Allocator,
    headers: [][]u8,
    rows: std.ArrayList([]u8),
    scan_done: bool = false,
    scan_error: bool = false,
    /// Sum of lengths of appended body lines (bytes).
    bytes_loaded: usize = 0,

    pub fn deinit(self: *PreviewData) void {
        for (self.headers) |h| self.allocator.free(h);
        self.allocator.free(self.headers);
        for (self.rows.items) |r| self.allocator.free(r);
        self.rows.deinit(self.allocator);
    }
};

pub fn empty(allocator: std.mem.Allocator) !PreviewData {
    return .{
        .mutex = .{},
        .allocator = allocator,
        .headers = try allocator.alloc([]u8, 0),
        .rows = std.ArrayList([]u8){},
        .scan_done = true,
        .scan_error = false,
        .bytes_loaded = 0,
    };
}

fn parseHeaderRow(allocator: std.mem.Allocator, header_slice: []const u8) ![][]u8 {
    var parsed_headers = try schema.splitRow(allocator, header_slice);
    defer parsed_headers.deinit();

    var headers = std.ArrayList([]u8){};
    defer headers.deinit(allocator);
    for (parsed_headers.items) |h| {
        try headers.append(allocator, try allocator.dupe(u8, h));
    }
    return try headers.toOwnedSlice(allocator);
}

/// Shared path reader: header + up to `max_body_lines` body rows. `scan_done` reflects whether the file is fully read.
fn loadFromPath(
    allocator: std.mem.Allocator,
    file_path: []const u8,
    max_body_lines: usize,
    scan_done_flag: bool,
) !PreviewData {
    var file = try std.fs.cwd().openFile(file_path, .{});
    defer file.close();

    var io_buf: [1024 * 1024]u8 = undefined;
    var file_reader = file.reader(&io_buf);

    const header_slice = (try std.Io.Reader.takeDelimiter(&file_reader.interface, '\n')) orelse {
        return try empty(allocator);
    };

    const headers = try parseHeaderRow(allocator, header_slice);

    var rows = std.ArrayList([]u8){};
    errdefer {
        for (rows.items) |r| allocator.free(r);
        rows.deinit(allocator);
    }
    var bytes_loaded: usize = 0;
    while (rows.items.len < max_body_lines) {
        const line = (try std.Io.Reader.takeDelimiter(&file_reader.interface, '\n')) orelse break;
        const owned = try allocator.dupe(u8, line);
        try rows.append(allocator, owned);
        bytes_loaded += owned.len;
    }

    return .{
        .mutex = .{},
        .allocator = allocator,
        .headers = headers,
        .rows = rows,
        .scan_done = scan_done_flag,
        .scan_error = false,
        .bytes_loaded = bytes_loaded,
    };
}

/// Header only; body filled later on a thread.
pub fn loadPreviewHeaderOnly(allocator: std.mem.Allocator, file_path: []const u8) !PreviewData {
    return loadFromPath(allocator, file_path, 0, false);
}

/// Header plus the first `initial_body_lines` body rows (sync). Rest via `streamAppendBodyLinesAfterSkip`.
pub fn loadPreviewHeaderAndInitialRows(
    allocator: std.mem.Allocator,
    file_path: []const u8,
    initial_body_lines: usize,
) !PreviewData {
    return loadFromPath(allocator, file_path, initial_body_lines, false);
}

/// Load header and up to `limit` body lines in one shot (benchmark / tests).
pub fn loadPreviewLimited(
    allocator: std.mem.Allocator,
    file_path: []const u8,
    limit: usize,
) !PreviewData {
    return loadFromPath(allocator, file_path, limit, true);
}

/// Background thread: open file, skip header + `skip_body_lines` data lines, append the rest to `preview.rows`.
pub fn streamAppendBodyLinesAfterSkip(
    preview: *PreviewData,
    file_path: []const u8,
    skip_body_lines: usize,
) void {
    var file = std.fs.cwd().openFile(file_path, .{}) catch {
        preview.mutex.lock();
        preview.scan_error = true;
        preview.scan_done = true;
        preview.mutex.unlock();
        return;
    };
    defer file.close();

    var io_buf: [1024 * 1024]u8 = undefined;
    var file_reader = file.reader(&io_buf);

    _ = std.Io.Reader.takeDelimiter(&file_reader.interface, '\n') catch {
        preview.mutex.lock();
        preview.scan_error = true;
        preview.scan_done = true;
        preview.mutex.unlock();
        return;
    };

    var skipped: usize = 0;
    while (skipped < skip_body_lines) : (skipped += 1) {
        const discard = (std.Io.Reader.takeDelimiter(&file_reader.interface, '\n') catch {
            preview.mutex.lock();
            preview.scan_error = true;
            preview.scan_done = true;
            preview.mutex.unlock();
            return;
        }) orelse {
            // EOF: sync path already consumed the whole file.
            preview.mutex.lock();
            preview.scan_done = true;
            preview.mutex.unlock();
            return;
        };
        _ = discard;
    }

    while (true) {
        const line = (std.Io.Reader.takeDelimiter(&file_reader.interface, '\n') catch {
            preview.mutex.lock();
            preview.scan_error = true;
            preview.mutex.unlock();
            break;
        }) orelse break;

        const owned = preview.allocator.dupe(u8, line) catch {
            preview.mutex.lock();
            preview.scan_error = true;
            preview.mutex.unlock();
            break;
        };

        preview.mutex.lock();
        preview.rows.append(preview.allocator, owned) catch {
            preview.allocator.free(owned);
            preview.scan_error = true;
            preview.mutex.unlock();
            break;
        };
        preview.bytes_loaded += owned.len;
        preview.mutex.unlock();
    }

    preview.mutex.lock();
    preview.scan_done = true;
    preview.mutex.unlock();
}

/// Same as `streamAppendBodyLinesAfterSkip(preview, path, 0)`.
pub fn streamAppendBodyLines(preview: *PreviewData, file_path: []const u8) void {
    streamAppendBodyLinesAfterSkip(preview, file_path, 0);
}

/// Optional: same as rendering each row — parse every loaded line with `splitRow` (extra CPU vs preview only).
pub fn parseAllLoadedRows(allocator: std.mem.Allocator, rows: []const []u8) !void {
    for (rows) |line| {
        var fields = try schema.splitRow(allocator, line);
        fields.deinit();
    }
}

test "preview load smoke (optional fixture)" {
    const path = "test-data/generated/test_1000x100.csv";
    std.fs.cwd().access(path, .{}) catch return error.SkipZigTest;

    const allocator = std.heap.page_allocator;

    var data = try loadPreviewLimited(allocator, path, 500);
    defer data.deinit();

    try std.testing.expect(data.headers.len > 0);
    try std.testing.expect(data.rows.items.len > 0);
}

test "benchmark preview load throughput" {
    const path = "test-data/generated/test_1000x100.csv";
    std.fs.cwd().access(path, .{}) catch return error.SkipZigTest;

    const allocator = std.heap.page_allocator;

    const limit: usize = 500;
    var timer = try std.time.Timer.start();
    var data = try loadPreviewLimited(allocator, path, limit);
    defer data.deinit();
    const elapsed_ns = timer.read();

    const total_bytes: u64 = blk: {
        var sum: u64 = 0;
        for (data.rows.items) |r| sum += @intCast(r.len);
        break :blk sum;
    };

    std.debug.print(
        "\n[bench] preview load: rows={d} cols={d} raw_bytes={d} time_ns={d}\n",
        .{ data.rows.items.len, data.headers.len, total_bytes, elapsed_ns },
    );

    try std.testing.expect(data.rows.items.len <= limit);
    try std.testing.expect(data.rows.items.len > 0);
}
