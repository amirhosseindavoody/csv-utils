//! Standalone benchmark: sync `preview.loadPreviewLimited` (header + N body lines). TUI uses header-only + background streaming instead.
//! Optional `--parse-fields`: after load, run `schema.splitRow` on every loaded line (extra cost vs preview only).

const std = @import("std");
const pv = @import("core/preview.zig");

const default_csv_path = "test-data/generated/test_1000x100.csv";

fn isAllDigits(s: []const u8) bool {
    if (s.len == 0) return false;
    for (s) |c| if (!std.ascii.isDigit(c)) return false;
    return true;
}

fn printUsage() void {
    std.debug.print(
        \\Usage: bench-csv-parse [[file.csv] [limit]] [--parse-fields]
        \\
        \\  No arguments: {s} with limit 500.
        \\  One argument: if numeric, that limit and default file; else path with limit 500.
        \\  Two arguments: file path then row limit.
        \\  Preview load = header parse + up to `limit` raw line reads (same as TUI startup).
        \\  --parse-fields  Also split every loaded data line into fields (heavier).
        \\
    , .{default_csv_path});
}

pub fn main() !void {
    const allocator = std.heap.page_allocator;

    var parse_fields = false;
    var pos1: ?[]const u8 = null;
    var pos2: ?[]const u8 = null;

    const argv = std.os.argv;
    var ai: usize = 1;
    while (ai < argv.len) : (ai += 1) {
        const arg = std.mem.span(argv[ai]);
        if (std.mem.eql(u8, arg, "--parse-fields")) {
            parse_fields = true;
            continue;
        }
        if (pos1 == null) {
            pos1 = arg;
            continue;
        }
        if (pos2 == null) {
            pos2 = arg;
            continue;
        }
        std.debug.print("Too many arguments.\n\n", .{});
        printUsage();
        return error.InvalidArgs;
    }

    var limit: usize = 500;
    const file_path: []const u8 = blk: {
        if (pos1 == null and pos2 == null) break :blk default_csv_path;
        if (pos1 != null and pos2 == null) {
            if (isAllDigits(pos1.?)) {
                limit = try std.fmt.parseInt(usize, pos1.?, 10);
                break :blk default_csv_path;
            }
            break :blk pos1.?;
        }
        if (pos1 != null and pos2 != null) {
            limit = try std.fmt.parseInt(usize, pos2.?, 10);
            break :blk pos1.?;
        }
        unreachable;
    };

    var timer = try std.time.Timer.start();
    var data = try pv.loadPreviewLimited(allocator, file_path, limit);
    defer data.deinit();
    const load_ns = timer.lap();

    var parse_ns: u64 = 0;
    if (parse_fields) {
        timer.reset();
        try pv.parseAllLoadedRows(allocator, data.rows.items);
        parse_ns = timer.read();
    }

    var raw_bytes: u64 = 0;
    for (data.rows.items) |r| raw_bytes += @intCast(r.len);

    std.debug.print(
        "preview_load: path={s} rows={d} header_cols={d} raw_body_bytes={d}\n",
        .{ file_path, data.rows.items.len, data.headers.len, raw_bytes },
    );
    std.debug.print("  load_time_ns={d}\n", .{load_ns});
    if (load_ns > 0) {
        const rate = (@as(u128, raw_bytes) * @as(u128, std.time.ns_per_s)) / @as(u128, load_ns);
        const mib_per_s_times_100 = @as(u64, @truncate((rate * 100) / (1024 * 1024)));
        std.debug.print("  approx_throughput_centi_MiB_s={d} (= {d}.{d} MiB/s)\n", .{
            mib_per_s_times_100,
            mib_per_s_times_100 / 100,
            mib_per_s_times_100 % 100,
        });
    }
    if (parse_fields) {
        std.debug.print("  splitRow_all_rows_ns={d}\n", .{parse_ns});
    }
}
