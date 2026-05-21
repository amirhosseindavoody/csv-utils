const std = @import("std");
const vaxis = @import("vaxis");
const schema = @import("../core/schema.zig");
const pv = @import("../core/preview.zig");

const Event = union(enum) {
    key_press: vaxis.Key,
    mouse: vaxis.Mouse,
    winsize: vaxis.Winsize,
    focus_in,
    focus_out,
};

const row_marker_width: i32 = 2;
const default_col_width: i32 = 16;
const min_col_width: i32 = 6;
const max_col_width: i32 = 64;
const min_sidebar_width: i32 = 18;
const max_sidebar_width: i32 = 60;

const ColumnKind = enum {
    str,
    long_str,
    float_general,
    float_scientific,
    float_mixed,
    int,
    date,
    unknown,
};

fn inferColumnKind(name: []const u8) ColumnKind {
    if (std.mem.startsWith(u8, name, "long_str_")) return .long_str;
    if (std.mem.startsWith(u8, name, "float_general_")) return .float_general;
    if (std.mem.startsWith(u8, name, "float_scientific_")) return .float_scientific;
    if (std.mem.startsWith(u8, name, "float_mixed_")) return .float_mixed;
    if (std.mem.startsWith(u8, name, "int_")) return .int;
    if (std.mem.startsWith(u8, name, "date_")) return .date;
    if (std.mem.startsWith(u8, name, "str_")) return .str;
    return .unknown;
}

fn columnKindLabel(kind: ColumnKind) []const u8 {
    return switch (kind) {
        .str => "str",
        .long_str => "long_str",
        .float_general => "float",
        .float_scientific => "float_sci",
        .float_mixed => "float_mix",
        .int => "int",
        .date => "date",
        .unknown => "?",
    };
}

fn isRightAligned(kind: ColumnKind) bool {
    return switch (kind) {
        .int, .float_general, .float_scientific, .float_mixed => true,
        else => false,
    };
}

fn ensureColumnWidths(widths: *std.ArrayList(i32), alloc: std.mem.Allocator, count: usize) !void {
    while (widths.items.len < count) {
        try widths.append(alloc, default_col_width);
    }
    if (widths.items.len > count) {
        widths.shrinkRetainingCapacity(count);
    }
}

fn colStartX(widths: []const i32, col_offset: usize, col: usize) i32 {
    var x: i32 = row_marker_width;
    var i = col_offset;
    while (i < col and i < widths.len) : (i += 1) {
        x += widths[i];
    }
    return x;
}

fn colBoundaryX(widths: []const i32, col_offset: usize, col: usize) i32 {
    return colStartX(widths, col_offset, col) + widths[col] - 1;
}

fn findBoundaryColumn(mx: i32, widths: []const i32, col_offset: usize, table_end: i32) ?usize {
    var col: usize = col_offset;
    while (col < widths.len) : (col += 1) {
        const boundary = colBoundaryX(widths, col_offset, col);
        if (colStartX(widths, col_offset, col) >= table_end) break;
        if (mx == boundary) return col;
        if (boundary >= table_end) break;
    }
    return null;
}

fn columnAtX(mx: i32, widths: []const i32, col_offset: usize, table_end: i32) ?usize {
    var col: usize = col_offset;
    while (col < widths.len) : (col += 1) {
        const start = colStartX(widths, col_offset, col);
        if (start >= table_end) break;
        const end = start + widths[col];
        // Last column char is the boundary separator, not cell content.
        if (mx >= start and mx < end - 1) return col;
        if (end >= table_end) break;
    }
    return null;
}

fn visibleColumnCount(widths: []const i32, col_offset: usize, table_end: i32) usize {
    var count: usize = 0;
    var col: usize = col_offset;
    while (col < widths.len) : (col += 1) {
        if (colStartX(widths, col_offset, col) >= table_end) break;
        count += 1;
        if (colBoundaryX(widths, col_offset, col) >= table_end) break;
    }
    return count;
}

fn updateColumnWidthAtMouse(col: usize, mx: i32, widths: []i32, col_offset: usize) void {
    const start = colStartX(widths, col_offset, col);
    const new_w = mx - start + 1;
    widths[col] = std.math.clamp(new_w, min_col_width, max_col_width);
}

pub fn run(io: std.Io, env_map: *std.process.Environ.Map, file_path: ?[]const u8) !void {
    var preview = if (file_path) |p|
        try pv.loadPreviewHeaderAndInitialRows(io, std.heap.page_allocator, p, 128)
    else
        try pv.empty(std.heap.page_allocator);
    defer preview.deinit();

    var scan_thread: ?std.Thread = null;
    if (file_path) |path| {
        const skip_loaded = preview.rows.items.len;
        scan_thread = try std.Thread.spawn(.{}, pv.streamAppendBodyLinesAfterSkip, .{ io, &preview, path, skip_loaded });
    }
    defer if (scan_thread) |t| t.join();

    var tty_buf: [4096]u8 = undefined;
    var tty = try vaxis.Tty.init(io, &tty_buf);
    defer tty.deinit();

    var vx = try vaxis.init(io, std.heap.page_allocator, env_map, .{});
    defer vx.deinit(std.heap.page_allocator, tty.writer());

    var loop: vaxis.Loop(Event) = .init(io, &tty, &vx);
    try loop.start();
    defer loop.stop();

    try vx.enterAltScreen(tty.writer());
    // Ensure terminal is in ASCII charset (not DEC special graphics mode),
    // otherwise regular letters can render as cryptic box-drawing symbols.
    try tty.writer().writeAll("\x1b(B\x1b)B\x0f");
    // Keep rendering in conservative defaults; some terminals mis-handle
    // capability-negotiated unicode mode and display garbled glyphs.
    try vx.setMouseMode(tty.writer(), true);
    if (!vx.state.in_band_resize) try loop.installResizeHandler();
    try vx.resize(std.heap.page_allocator, tty.writer(), try tty.getWinsize());

    var selected_row: usize = 0;
    var selected_col: usize = 0;
    var row_offset: usize = 0;
    var col_offset: usize = 0;
    var sidebar_width: i32 = 28;
    var sidebar_col_offset: usize = 0;
    var resizing_sidebar = false;
    var resizing_col: ?usize = null;
    var show_column_types = false;
    var column_widths: std.ArrayList(i32) = .empty;
    defer column_widths.deinit(std.heap.page_allocator);
    var running = true;

    while (running) {
        lock(&preview.mutex);
        const header_count = preview.headers.len;
        unlock(&preview.mutex);
        ensureColumnWidths(&column_widths, std.heap.page_allocator, header_count) catch break;

        var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
        defer arena.deinit();
        draw(&vx, &preview, file_path, selected_row, selected_col, row_offset, col_offset, column_widths.items, sidebar_width, sidebar_col_offset, resizing_sidebar, resizing_col, show_column_types, arena.allocator());
        try vx.render(tty.writer());

        const ev = try loop.nextEvent();
        switch (ev) {
            .winsize => |ws| try vx.resize(std.heap.page_allocator, tty.writer(), ws),
            .key_press => |k| {
                if (k.matches('q', .{})) running = false;
                if (k.matches(vaxis.Key.up, .{})) selected_row = selected_row -| 1;
                if (k.matches(vaxis.Key.down, .{})) selected_row += 1;
                if (k.matches(vaxis.Key.left, .{})) selected_col = selected_col -| 1;
                if (k.matches(vaxis.Key.right, .{})) selected_col += 1;
                if (k.matches('t', .{})) show_column_types = !show_column_types;
            },
            .mouse => |m| {
                const screen_w: i32 = @intCast(vx.screen.width);
                const sidebar_x = screen_w - sidebar_width;
                const splitter_x = sidebar_x - 1;
                const table_end = screen_w - sidebar_width - 1;
                const mx: i32 = @intCast(m.col);
                const my: i32 = @intCast(m.row);
                const in_table = my >= 3 and my < @as(i32, @intCast(vx.screen.height)) - 1 and mx >= row_marker_width and mx < table_end;
                const in_sidebar_body = mx >= sidebar_x and my >= 4 and my < @as(i32, @intCast(vx.screen.height)) - 1;

                if (m.button == .left and m.type == .release) {
                    resizing_sidebar = false;
                    resizing_col = null;
                } else if (resizing_col) |col| {
                    if (m.type == .motion or m.type == .drag) {
                        updateColumnWidthAtMouse(col, mx, column_widths.items, col_offset);
                    }
                } else if (resizing_sidebar and (m.type == .motion or m.type == .drag)) {
                    updateSidebarWidth(mx, screen_w, &sidebar_width);
                } else if (m.button == .wheel_up and in_sidebar_body) {
                    sidebar_col_offset = sidebar_col_offset -| 1;
                } else if (m.button == .wheel_down and in_sidebar_body) {
                    sidebar_col_offset += 1;
                } else if (m.button == .wheel_up and m.type == .press) {
                    selected_row = selected_row -| 3;
                } else if (m.button == .wheel_down and m.type == .press) {
                    selected_row += 3;
                } else if (m.button == .left and m.type == .press) {
                    if (mx == splitter_x) {
                        resizing_sidebar = true;
                        resizing_col = null;
                    } else if (in_sidebar_body) {
                        applySidebarMouseSelection(m, &selected_col, sidebar_col_offset);
                    } else if (in_table) {
                        if (findBoundaryColumn(mx, column_widths.items, col_offset, table_end)) |col| {
                            resizing_col = col;
                            resizing_sidebar = false;
                        } else {
                            applyMouseSelection(m, &selected_row, &selected_col, row_offset, column_widths.items, col_offset, table_end);
                        }
                    }
                }
            },
            else => {},
        }

        lock(&preview.mutex);
        const max_rows = if (preview.rows.items.len == 0) 0 else preview.rows.items.len - 1;
        const max_cols = if (preview.headers.len == 0) 0 else preview.headers.len - 1;
        unlock(&preview.mutex);
        if (selected_row > max_rows) selected_row = max_rows;
        if (selected_col > max_cols) selected_col = max_cols;

        const vr: usize = @intCast(if (@as(i32, @intCast(vx.screen.height)) - 5 > 0) @as(i32, @intCast(vx.screen.height)) - 5 else 0);
        if (selected_row < row_offset) row_offset = selected_row;
        if (vr > 0 and selected_row >= row_offset + vr) row_offset = selected_row - vr + 1;
        const table_end: i32 = @as(i32, @intCast(vx.screen.width)) - sidebar_width - 1;
        const vc = visibleColumnCount(column_widths.items, col_offset, table_end);
        if (selected_col < col_offset) col_offset = selected_col;
        if (vc > 0 and selected_col >= col_offset + vc) col_offset = selected_col - vc + 1;

        lock(&preview.mutex);
        const max_sidebar_off = if (preview.headers.len > 0) preview.headers.len - 1 else 0;
        unlock(&preview.mutex);
        if (sidebar_col_offset > max_sidebar_off) sidebar_col_offset = max_sidebar_off;
    }
}

fn draw(vx: *vaxis.Vaxis, preview: *pv.PreviewData, file_path: ?[]const u8, selected_row: usize, selected_col: usize, row_offset: usize, col_offset: usize, column_widths: []const i32, sidebar_width: i32, sidebar_col_offset: usize, resizing_sidebar: bool, resizing_col: ?usize, show_column_types: bool, alloc: std.mem.Allocator) void {
    lock(&preview.mutex);
    defer unlock(&preview.mutex);

    const win = vx.window();
    win.clear();
    win.hideCursor();

    print(win, 0, 0, "csv-utils TUI (libvaxis)", .{});
    var buf: [256]u8 = undefined;
    const line = std.fmt.bufPrint(&buf, "file={s} rows={d} cell=[r{d},c{d}]", .{
        file_path orelse "<none>",
        preview.rows.items.len,
        selected_row + 1,
        selected_col + 1,
    }) catch "";
    print(win, 0, 1, line, .{});
    hline(win, 2, '-', @intCast(vx.screen.width));

    drawHeaders(win, preview.headers, selected_col, col_offset, column_widths, sidebar_width, resizing_col, alloc);
    drawRows(win, preview, selected_row, selected_col, row_offset, col_offset, column_widths, sidebar_width, resizing_col, alloc);
    drawSidebar(win, preview.headers, selected_col, sidebar_col_offset, sidebar_width, resizing_sidebar, show_column_types, alloc);
    const sh: i32 = @intCast(vx.screen.height);
    const sw: i32 = @intCast(vx.screen.width);
    hline(win, sh - 1, '-', sw);
}

/// Draw the header row (fixed top row) of the table.
fn drawHeaders(win: vaxis.Window, headers: [][]u8, selected_col: usize, col_offset: usize, column_widths: []const i32, sidebar_width: i32, resizing_col: ?usize, alloc: std.mem.Allocator) void {
    const table_end: i32 = @as(i32, @intCast(win.width)) - sidebar_width - 1;
    var col: usize = col_offset;
    while (col < headers.len and col < column_widths.len) : (col += 1) {
        const x = colStartX(column_widths, col_offset, col);
        if (x >= table_end) break;
        const w = column_widths[col];
        const kind = inferColumnKind(headers[col]);
        drawCell(win, x, 3, headers[col], w, isRightAligned(kind), if (col == selected_col) .{ .reverse = true, .bold = true } else .{ .bold = true }, alloc);
        const sep_style: vaxis.Style = if (resizing_col == col) .{ .reverse = true, .bold = true } else .{};
        print(win, x + w - 1, 3, "|", sep_style);
        if (x + w >= table_end) break;
    }
}

/// Draw the visible rows of the table.
fn drawRows(win: vaxis.Window, preview: *pv.PreviewData, selected_row: usize, selected_col: usize, row_offset: usize, col_offset: usize, column_widths: []const i32, sidebar_width: i32, resizing_col: ?usize, alloc: std.mem.Allocator) void {
    const sh: i32 = @intCast(win.height);
    const table_end: i32 = @as(i32, @intCast(win.width)) - sidebar_width - 1;
    const vis: usize = @intCast(if (sh - 5 > 0) sh - 5 else 0);
    var i: usize = 0;
    while (i < vis and row_offset + i < preview.rows.items.len) : (i += 1) {
        const row_idx = row_offset + i;
        const y = 4 + @as(i32, @intCast(i));
        print(win, 0, y, if (row_idx == selected_row) ">" else " ", .{});
        var fields = schema.splitRow(alloc, preview.rows.items[row_idx]) catch return;
        defer fields.deinit();

        var col: usize = col_offset;
        while (col < preview.headers.len and col < column_widths.len) : (col += 1) {
            const x = colStartX(column_widths, col_offset, col);
            if (x >= table_end) break;
            const w = column_widths[col];
            const txt = if (col < fields.items.len) fields.items[col] else "";
            const kind = inferColumnKind(preview.headers[col]);
            drawCell(win, x, y, txt, w, isRightAligned(kind), if (row_idx == selected_row and col == selected_col) .{ .reverse = true } else .{}, alloc);
            const sep_style: vaxis.Style = if (resizing_col == col) .{ .reverse = true, .bold = true } else .{};
            print(win, x + w - 1, y, "|", sep_style);
            if (x + w >= table_end) break;
        }
    }
}

/// Draw the sidebar (fixed left column) of the table.
fn drawSidebar(win: vaxis.Window, headers: [][]u8, selected_col: usize, sidebar_col_offset: usize, sidebar_width: i32, resizing_sidebar: bool, show_column_types: bool, alloc: std.mem.Allocator) void {
    const sh: i32 = @intCast(win.height);
    const sidebar_x = @as(i32, @intCast(win.width)) - sidebar_width;
    const sidebar_content_x = sidebar_x;
    const sidebar_content_w: usize = @intCast(sidebar_width);
    const splitter_x = sidebar_x - 1;
    const vis: usize = @intCast(if (sh - 5 > 0) sh - 5 else 0);

    const splitter_style: vaxis.Style = if (resizing_sidebar) .{ .reverse = true, .bold = true } else .{ .bold = true };
    if (splitter_x - 1 >= 0) print(win, splitter_x - 1, 3, "<>", splitter_style);
    const title = if (show_column_types) "Columns (t)" else "Columns";
    print(win, sidebar_content_x, 3, title, .{ .bold = true });

    var i: usize = 0;
    while (i < vis and sidebar_col_offset + i < headers.len) : (i += 1) {
        const col_idx = sidebar_col_offset + i;
        const y = 4 + @as(i32, @intCast(i));

        var line_buf: [96]u8 = undefined;
        const kind = inferColumnKind(headers[col_idx]);
        const line = if (show_column_types)
            std.fmt.bufPrint(&line_buf, "{d}: {s} [{s}]", .{ col_idx + 1, headers[col_idx], columnKindLabel(kind) }) catch headers[col_idx]
        else
            std.fmt.bufPrint(&line_buf, "{d}: {s}", .{ col_idx + 1, headers[col_idx] }) catch headers[col_idx];
        drawSidebarCell(win, sidebar_content_x, y, line, if (col_idx == selected_col) .{ .reverse = true } else .{}, sidebar_content_w, alloc);
    }
}

/// Draw a single cell in the table.
fn drawCell(win: vaxis.Window, x: i32, y: i32, text: []const u8, col_w: i32, align_right: bool, style: vaxis.Style, alloc: std.mem.Allocator) void {
    const w: usize = @intCast(@max(col_w - 1, 0));
    const buf = alloc.alloc(u8, w) catch return;
    @memset(buf, ' ');
    if (buf.len == 0) return;

    const truncated = text.len > buf.len;
    const take = @min(text.len, buf.len);
    var scratch: [32]u8 = undefined;
    if (take > scratch.len) return;

    var i: usize = 0;
    while (i < take) : (i += 1) {
        const b = text[i];
        scratch[i] = if (b >= 32 and b <= 126) b else '.';
    }
    const vis_len = if (truncated) buf.len else take;

    if (align_right) {
        const start = buf.len - vis_len;
        @memcpy(buf[start..][0..vis_len], scratch[0..vis_len]);
    } else {
        @memcpy(buf[0..vis_len], scratch[0..vis_len]);
    }
    if (truncated) buf[buf.len - 1] = '~';
    print(win, x, y, buf, style);
}

/// Apply mouse selection to the table.
fn applyMouseSelection(
    m: vaxis.Mouse,
    selected_row: *usize,
    selected_col: *usize,
    row_offset: usize,
    widths: []const i32,
    col_offset: usize,
    table_end: i32,
) void {
    const mx: i32 = @intCast(m.col);
    const my: i32 = @intCast(m.row);
    if (mx < row_marker_width or mx >= table_end) return;

    const col = columnAtX(mx, widths, col_offset, table_end) orelse return;
    selected_col.* = col;

    if (my < 4) return;
    const row_rel = my - 4;
    selected_row.* = row_offset + @as(usize, @intCast(row_rel));
}

/// Apply mouse selection to the sidebar.
fn applySidebarMouseSelection(
    m: vaxis.Mouse,
    selected_col: *usize,
    sidebar_col_offset: usize,
) void {
    const my: i32 = @intCast(m.row);
    if (my < 4) return;
    const row_rel = my - 4;
    if (row_rel < 0) return;
    selected_col.* = sidebar_col_offset + @as(usize, @intCast(row_rel));
}

fn updateSidebarWidth(mouse_x: i32, screen_w: i32, sidebar_width: *i32) void {
    const desired = screen_w - mouse_x;
    sidebar_width.* = std.math.clamp(desired, min_sidebar_width, @min(max_sidebar_width, screen_w - 20));
}

fn drawSidebarCell(win: vaxis.Window, x: i32, y: i32, text: []const u8, style: vaxis.Style, width: usize, alloc: std.mem.Allocator) void {
    if (width == 0) return;
    const buf = alloc.alloc(u8, width) catch return;
    @memset(buf, ' ');
    const take = @min(text.len, buf.len);
    var i: usize = 0;
    while (i < take) : (i += 1) {
        const b = text[i];
        buf[i] = if (b >= 32 and b <= 126) b else '.';
    }
    if (text.len > buf.len and buf.len > 0) buf[buf.len - 1] = '~';
    print(win, x, y, buf, style);
}

fn lock(m: *std.atomic.Mutex) void {
    while (!m.tryLock()) std.Thread.yield() catch {};
}

fn unlock(m: *std.atomic.Mutex) void {
    m.unlock();
}

fn print(win: vaxis.Window, x: i32, y: i32, text: []const u8, style: vaxis.Style) void {
    _ = win.printSegment(.{ .text = text, .style = style }, .{
        .col_offset = @intCast(@max(x, 0)),
        .row_offset = @intCast(@max(y, 0)),
        .wrap = .none,
    });
}

fn hline(win: vaxis.Window, y: i32, ch: u8, w: i32) void {
    var i: i32 = 0;
    while (i < w) : (i += 1) {
        var b: [1]u8 = .{ch};
        print(win, i, y, b[0..], .{});
    }
}

fn vline(win: vaxis.Window, x: i32, y0: i32, h: i32, ch: u8, style: vaxis.Style) void {
    var i: i32 = 0;
    while (i < h) : (i += 1) {
        var b: [1]u8 = .{ch};
        print(win, x, y0 + i, b[0..], style);
    }
}
