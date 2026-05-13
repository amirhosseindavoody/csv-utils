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
const col_width: i32 = 16;
const min_sidebar_width: i32 = 18;
const max_sidebar_width: i32 = 60;

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
    var running = true;

    while (running) {
        var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
        defer arena.deinit();
        draw(&vx, &preview, file_path, selected_row, selected_col, row_offset, col_offset, sidebar_width, sidebar_col_offset, resizing_sidebar, arena.allocator());
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
            },
            .mouse => |m| {
                const sidebar_x = @as(i32, @intCast(vx.screen.width)) - sidebar_width;
                const splitter_x = sidebar_x - 1;
                const mx: i32 = @intCast(m.col);
                const my: i32 = @intCast(m.row);
                const in_sidebar_body = mx >= sidebar_x and my >= 4 and my < @as(i32, @intCast(vx.screen.height)) - 1;

                if (m.button == .left and m.type == .press and mx == splitter_x) {
                    resizing_sidebar = true;
                } else if (m.button == .left and m.type == .release) {
                    resizing_sidebar = false;
                } else if (resizing_sidebar and (m.type == .motion or m.type == .drag)) {
                    updateSidebarWidth(mx, @intCast(vx.screen.width), &sidebar_width);
                } else if (m.button == .wheel_up and in_sidebar_body) {
                    sidebar_col_offset = sidebar_col_offset -| 1;
                } else if (m.button == .wheel_down and in_sidebar_body) {
                    sidebar_col_offset += 1;
                } else if (m.button == .wheel_up and m.type == .press) {
                    selected_row = selected_row -| 3;
                } else if (m.button == .wheel_down and m.type == .press) {
                    selected_row += 3;
                } else if (m.button == .left and m.type == .press) {
                    if (mx >= sidebar_x and my >= 4 and my < @as(i32, @intCast(vx.screen.height)) - 1) {
                        applySidebarMouseSelection(m, &selected_col, sidebar_col_offset);
                    } else {
                        applyMouseSelection(m, &selected_row, &selected_col, row_offset, col_offset, sidebar_width, @intCast(vx.screen.width));
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
        const table_w = @as(i32, @intCast(vx.screen.width)) - sidebar_width - 1;
        const vc: usize = @intCast(if (table_w - row_marker_width > 0) @divFloor(table_w - row_marker_width, col_width) else 0);
        if (selected_col < col_offset) col_offset = selected_col;
        if (vc > 0 and selected_col >= col_offset + vc) col_offset = selected_col - vc + 1;

        lock(&preview.mutex);
        const max_sidebar_off = if (preview.headers.len > 0) preview.headers.len - 1 else 0;
        unlock(&preview.mutex);
        if (sidebar_col_offset > max_sidebar_off) sidebar_col_offset = max_sidebar_off;
    }
}

fn draw(vx: *vaxis.Vaxis, preview: *pv.PreviewData, file_path: ?[]const u8, selected_row: usize, selected_col: usize, row_offset: usize, col_offset: usize, sidebar_width: i32, sidebar_col_offset: usize, resizing_sidebar: bool, alloc: std.mem.Allocator) void {
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

    drawHeaders(win, preview.headers, selected_col, col_offset, sidebar_width, alloc);
    drawRows(win, preview, selected_row, selected_col, row_offset, col_offset, sidebar_width, alloc);
    drawSidebar(win, preview.headers, selected_col, sidebar_col_offset, sidebar_width, resizing_sidebar, alloc);
    const sh: i32 = @intCast(vx.screen.height);
    const sw: i32 = @intCast(vx.screen.width);
    hline(win, sh - 1, '-', sw);
}

/// Draw the header row (fixed top row) of the table.
fn drawHeaders(win: vaxis.Window, headers: [][]u8, selected_col: usize, col_offset: usize, sidebar_width: i32, alloc: std.mem.Allocator) void {
    var x: i32 = row_marker_width;
    var col: usize = col_offset;
    const table_end: i32 = @as(i32, @intCast(win.width)) - sidebar_width - 1;
    while (col < headers.len and x + col_width <= table_end) : (col += 1) {
        drawCell(win, x, 3, headers[col], if (col == selected_col) .{ .reverse = true, .bold = true } else .{ .bold = true }, alloc);
        print(win, x + col_width - 1, 3, "|", .{});
        x += col_width;
    }
}

/// Draw the visible rows of the table.
fn drawRows(win: vaxis.Window, preview: *pv.PreviewData, selected_row: usize, selected_col: usize, row_offset: usize, col_offset: usize, sidebar_width: i32, alloc: std.mem.Allocator) void {
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

        var x: i32 = row_marker_width;
        var col: usize = col_offset;
        while (col < preview.headers.len and x + col_width <= table_end) : (col += 1) {
            const txt = if (col < fields.items.len) fields.items[col] else "";
            drawCell(win, x, y, txt, if (row_idx == selected_row and col == selected_col) .{ .reverse = true } else .{}, alloc);
            print(win, x + col_width - 1, y, "|", .{});
            x += col_width;
        }
    }
}

/// Draw the sidebar (fixed left column) of the table.
fn drawSidebar(win: vaxis.Window, headers: [][]u8, selected_col: usize, sidebar_col_offset: usize, sidebar_width: i32, resizing_sidebar: bool, alloc: std.mem.Allocator) void {
    const sh: i32 = @intCast(win.height);
    const sidebar_x = @as(i32, @intCast(win.width)) - sidebar_width;
    const sidebar_content_x = sidebar_x;
    const sidebar_content_w: usize = @intCast(sidebar_width);
    const splitter_x = sidebar_x - 1;
    const vis: usize = @intCast(if (sh - 5 > 0) sh - 5 else 0);

    const splitter_style: vaxis.Style = if (resizing_sidebar) .{ .reverse = true, .bold = true } else .{ .bold = true };
    if (splitter_x - 1 >= 0) print(win, splitter_x - 1, 3, "<>", splitter_style);
    print(win, sidebar_content_x, 3, "Columns", .{ .bold = true });

    var i: usize = 0;
    while (i < vis and sidebar_col_offset + i < headers.len) : (i += 1) {
        const col_idx = sidebar_col_offset + i;
        const y = 4 + @as(i32, @intCast(i));

        var line_buf: [64]u8 = undefined;
        const line = std.fmt.bufPrint(&line_buf, "{d}: {s}", .{ col_idx + 1, headers[col_idx] }) catch headers[col_idx];
        drawSidebarCell(win, sidebar_content_x, y, line, if (col_idx == selected_col) .{ .reverse = true } else .{}, sidebar_content_w, alloc);
    }
}

/// Draw a single cell in the table.
fn drawCell(win: vaxis.Window, x: i32, y: i32, text: []const u8, style: vaxis.Style, alloc: std.mem.Allocator) void {
    const w: usize = @intCast(col_width - 1);
    const buf = alloc.alloc(u8, w) catch return;
    @memset(buf, ' ');
    if (buf.len == 0) return;
    const take = @min(text.len, buf.len);
    var i: usize = 0;
    while (i < take) : (i += 1) {
        const b = text[i];
        buf[i] = if (b >= 32 and b <= 126) b else '.';
    }
    if (text.len > buf.len) buf[buf.len - 1] = '~';
    print(win, x, y, buf, style);
}

/// Apply mouse selection to the table.
fn applyMouseSelection(
    m: vaxis.Mouse,
    selected_row: *usize,
    selected_col: *usize,
    row_offset: usize,
    col_offset: usize,
    sidebar_width: i32,
    screen_width: i32,
) void {
    const mx: i32 = @intCast(m.col);
    const my: i32 = @intCast(m.row);
    if (my < 4) return;
    if (mx < row_marker_width) return;
    const table_end = screen_width - sidebar_width - 1;
    if (mx >= table_end) return;

    const col_rel = @divFloor(mx - row_marker_width, col_width);
    if (col_rel < 0) return;

    const row_rel = my - 4;
    if (row_rel < 0) return;

    selected_row.* = row_offset + @as(usize, @intCast(row_rel));
    selected_col.* = col_offset + @as(usize, @intCast(col_rel));
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
