const std = @import("std");
const schema = @import("../core/schema.zig");
const pv = @import("../core/preview.zig");
const c = @cImport({
    @cInclude("ncurses.h");
});

const row_marker_width: c_int = 2;
const col_width: c_int = 16;

pub fn run(file_path: ?[]const u8) !void {
    _ = c.initscr();
    defer _ = c.endwin();
    _ = c.cbreak();
    _ = c.noecho();
    _ = c.keypad(c.stdscr, true);
    _ = c.nodelay(c.stdscr, true);
    _ = c.curs_set(0);
    _ = c.timeout(100);
    if (c.has_colors()) {
        _ = c.start_color();
        _ = c.use_default_colors();
        _ = c.init_pair(1, c.COLOR_CYAN, -1);
        // Opaque panel: explicit background so underlying cells do not show through.
        _ = c.init_pair(2, c.COLOR_WHITE, c.COLOR_BLUE);
    }

    var csv = if (file_path) |p|
        try pv.loadPreviewHeaderAndInitialRows(std.heap.page_allocator, p, initialSyncBodyLines())
    else
        try pv.empty(std.heap.page_allocator);
    defer csv.deinit();

    var scan_thread: ?std.Thread = null;
    if (file_path) |path| {
        const skip_synced_body: usize = csv.rows.items.len;
        scan_thread = try std.Thread.spawn(.{}, pv.streamAppendBodyLinesAfterSkip, .{ &csv, path, skip_synced_body });
    }
    defer if (scan_thread) |t| t.join();

    var selected_row: usize = 0;
    var selected_col: usize = 0;
    var row_offset: usize = 0;
    var col_offset: usize = 0;
    var show_row_panel = false;
    var panel_scroll_row: usize = 0;
    var panel_scroll_col: usize = 0;
    var running = true;
    while (running) {
        var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
        defer arena.deinit();
        drawUi(
            file_path,
            &csv,
            selected_row,
            selected_col,
            row_offset,
            col_offset,
            show_row_panel,
            panel_scroll_row,
            panel_scroll_col,
            arena.allocator(),
        );
        const ch = c.getch();

        csv.mutex.lock();
        const n_loaded = csv.rows.items.len;
        csv.mutex.unlock();
        const max_rows: usize = if (n_loaded == 0) 0 else n_loaded - 1;
        const max_cols: usize = if (csv.headers.len == 0) 0 else csv.headers.len - 1;
        switch (ch) {
            'q' => running = false,
            'r' => {
                show_row_panel = !show_row_panel;
                panel_scroll_row = 0;
                panel_scroll_col = 0;
            },
            c.KEY_UP => {
                if (show_row_panel) {
                    panel_scroll_row = panel_scroll_row -| 1;
                } else if (selected_row > 0) {
                    selected_row -= 1;
                }
            },
            c.KEY_DOWN => {
                if (show_row_panel) {
                    panel_scroll_row += 1;
                } else if (selected_row < max_rows) {
                    selected_row += 1;
                }
            },
            c.KEY_LEFT => {
                if (show_row_panel) {
                    panel_scroll_col = panel_scroll_col -| 1;
                } else if (selected_col > 0) {
                    selected_col -= 1;
                }
            },
            c.KEY_RIGHT => {
                if (show_row_panel) {
                    panel_scroll_col += 1;
                } else if (selected_col < max_cols) {
                    selected_col += 1;
                }
            },
            c.KEY_NPAGE => {
                if (show_row_panel) {
                    panel_scroll_row += 10;
                } else {
                    const page = visibleBodyRows();
                    selected_row = @min(max_rows, selected_row + page);
                }
            },
            c.KEY_PPAGE => {
                if (show_row_panel) {
                    panel_scroll_row = panel_scroll_row -| 10;
                } else {
                    const page = visibleBodyRows();
                    selected_row = selected_row -| page;
                }
            },
            c.KEY_HOME => {
                if (show_row_panel) {
                    panel_scroll_row = 0;
                    panel_scroll_col = 0;
                } else {
                    selected_row = 0;
                    selected_col = 0;
                }
            },
            c.KEY_END => {
                if (!show_row_panel) selected_row = max_rows;
            },
            else => {},
        }

        if (!show_row_panel) {
            const visible_rows = visibleBodyRows();
            if (selected_row < row_offset) row_offset = selected_row;
            if (visible_rows > 0 and selected_row >= row_offset + visible_rows) {
                row_offset = selected_row - visible_rows + 1;
            }

            const visible_cols = visibleColumnCount();
            if (selected_col < col_offset) col_offset = selected_col;
            if (visible_cols > 0 and selected_col >= col_offset + visible_cols) {
                col_offset = selected_col - visible_cols + 1;
            }
        }
    }
}

fn drawUi(
    file_path: ?[]const u8,
    preview: *pv.PreviewData,
    selected_row: usize,
    selected_col: usize,
    row_offset: usize,
    col_offset: usize,
    show_row_panel: bool,
    panel_scroll_row: usize,
    panel_scroll_col: usize,
    temp_allocator: std.mem.Allocator,
) void {
    preview.mutex.lock();
    defer preview.mutex.unlock();

    _ = c.erase();

    const h = c.getmaxy(c.stdscr);
    const w = c.getmaxx(c.stdscr);

    _ = c.mvaddstr(0, 0, "csv-utils TUI (q quit, arrows navigate, r row panel)");

    var buffer: [320]u8 = undefined;
    const loaded = preview.rows.items.len;
    const bytes_l = preview.bytes_loaded;
    const status = if (preview.scan_error)
        "error"
    else if (preview.scan_done)
        "complete"
    else if (file_path != null)
        "loading"
    else
        "idle";
    const path = file_path orelse "<not provided>";
    const summary = std.fmt.bufPrintZ(
        &buffer,
        "file={s} scan={s} rows={d} bytes={d} cell=[r{d},c{d}] col_off={d}",
        .{ path, status, loaded, bytes_l, selected_row + 1, selected_col + 1, col_offset },
    ) catch "status unavailable";
    _ = c.mvaddstr(1, 0, summary.ptr);
    _ = c.mvhline(2, 0, '-', w);
    drawHeaderRow(3, w, preview.headers, selected_col, col_offset, temp_allocator);

    const visible_rows = visibleBodyRows();
    var screen_row: c_int = 4;
    var i: usize = 0;
    while (i < visible_rows and row_offset + i < loaded) : (i += 1) {
        const row_idx = row_offset + i;
        drawDataRow(
            screen_row,
            w,
            preview.headers.len,
            preview.rows.items[row_idx],
            row_idx == selected_row,
            selected_col,
            col_offset,
            temp_allocator,
        );
        screen_row += 1;
    }

    if (show_row_panel) {
        drawRowPanel(
            h,
            w,
            preview,
            selected_row,
            selected_col,
            panel_scroll_row,
            panel_scroll_col,
            temp_allocator,
        );
    }

    _ = c.mvhline(h - 1, 0, '-', w);
    _ = c.refresh();
}

fn drawHeaderRow(
    y: c_int,
    max_width: c_int,
    headers: [][]u8,
    selected_col: usize,
    col_offset: usize,
    temp_allocator: std.mem.Allocator,
) void {
    if (c.has_colors()) {
        _ = c.attron(c.COLOR_PAIR(1) | c.A_BOLD);
    } else {
        _ = c.attron(c.A_BOLD);
    }

    var x: c_int = row_marker_width;
    var col: usize = col_offset;
    while (col < headers.len and x + col_width <= max_width) : (col += 1) {
        const cell = formatCell(temp_allocator, headers[col], @intCast(col_width - 1)) catch return;
        if (col == selected_col) _ = c.attron(c.A_REVERSE);
        _ = c.mvaddnstr(y, x, cell.ptr, @intCast(cell.len));
        if (col == selected_col) _ = c.attroff(c.A_REVERSE);
        _ = c.mvaddch(y, x + col_width - 1, '|');
        x += col_width;
    }

    if (c.has_colors()) {
        _ = c.attroff(c.COLOR_PAIR(1) | c.A_BOLD);
    } else {
        _ = c.attroff(c.A_BOLD);
    }
}

fn drawDataRow(
    y: c_int,
    max_width: c_int,
    header_count: usize,
    row_line: []const u8,
    selected: bool,
    selected_col: usize,
    col_offset: usize,
    temp_allocator: std.mem.Allocator,
) void {
    var fields = schema.splitRow(temp_allocator, row_line) catch return;
    defer fields.deinit();

    _ = c.mvaddch(y, 0, if (selected) '>' else ' ');
    _ = c.mvaddch(y, 1, ' ');
    var x: c_int = row_marker_width;
    var col: usize = col_offset;
    while (col < header_count and x + col_width <= max_width) : (col += 1) {
        const cell = if (col < fields.items.len) fields.items[col] else "";
        const shown = formatCell(temp_allocator, cell, @intCast(col_width - 1)) catch return;
        if (selected and col == selected_col) _ = c.attron(c.A_REVERSE);
        _ = c.mvaddnstr(y, x, shown.ptr, @intCast(shown.len));
        if (selected and col == selected_col) _ = c.attroff(c.A_REVERSE);
        _ = c.mvaddch(y, x + col_width - 1, '|');
        x += col_width;
    }
}

fn formatCell(allocator: std.mem.Allocator, input: []const u8, width: usize) ![]const u8 {
    const out = try allocator.alloc(u8, width);
    @memset(out, ' ');
    if (width == 0) return out;
    if (input.len <= width) {
        @memcpy(out[0..input.len], input);
        return out;
    }
    const copy_len = width - 1;
    @memcpy(out[0..copy_len], input[0..copy_len]);
    out[copy_len] = '~';
    return out;
}

fn drawRowPanel(
    screen_h: c_int,
    screen_w: c_int,
    preview: *pv.PreviewData,
    selected_row: usize,
    selected_col: usize,
    scroll_row: usize,
    scroll_col: usize,
    temp_allocator: std.mem.Allocator,
) void {
    const panel_h: c_int = if (screen_h > 8) screen_h - 6 else 3;
    const panel_w: c_int = if (screen_w > 8) screen_w - 4 else screen_w;
    const panel_y: c_int = 3;
    const panel_x: c_int = 2;

    const inner_w = panel_w - 2;
    if (inner_w <= 0 or panel_h < 2) return;

    // Solid opaque layer: fill interior first so table/grid underneath never bleeds through.
    if (c.has_colors()) {
        _ = c.attron(c.COLOR_PAIR(2));
    } else {
        _ = c.attron(c.A_REVERSE);
    }
    var fill_y: c_int = panel_y + 1;
    while (fill_y < panel_y + panel_h - 1) : (fill_y += 1) {
        _ = c.mvhline(fill_y, panel_x + 1, ' ', inner_w);
    }
    if (c.has_colors()) {
        _ = c.attroff(c.COLOR_PAIR(2));
    } else {
        _ = c.attroff(c.A_REVERSE);
    }

    // Borders and chrome use the same opaque styling.
    if (c.has_colors()) {
        _ = c.attron(c.COLOR_PAIR(2));
    } else {
        _ = c.attron(c.A_REVERSE);
    }
    _ = c.mvhline(panel_y, panel_x, '=', panel_w);
    _ = c.mvhline(panel_y + panel_h - 1, panel_x, '=', panel_w);
    var i: c_int = 0;
    while (i < panel_h) : (i += 1) {
        _ = c.mvaddch(panel_y + i, panel_x, '|');
        _ = c.mvaddch(panel_y + i, panel_x + panel_w - 1, '|');
    }
    _ = c.mvaddstr(panel_y, panel_x + 2, "Row View Panel (r to close)");

    const content_h: usize = @intCast(if (panel_h > 2) panel_h - 2 else 0);
    const content_w: usize = @intCast(if (panel_w > 4) panel_w - 4 else 0);

    if (preview.rows.items.len == 0 or selected_row >= preview.rows.items.len) {
        if (content_w > 0 and content_h > 0) {
            _ = c.mvaddnstr(panel_y + 1, panel_x + 2, "{}", @intCast(@min(2, content_w)));
            _ = c.mvhline(panel_y + 1, panel_x + 2 + 2, ' ', @intCast(@max(0, @as(c_int, @intCast(content_w)) - 2)));
        }
        if (c.has_colors()) {
            _ = c.attroff(c.COLOR_PAIR(2));
        } else {
            _ = c.attroff(c.A_REVERSE);
        }
        return;
    }

    var fields = schema.splitRow(temp_allocator, preview.rows.items[selected_row]) catch {
        if (c.has_colors()) {
            _ = c.attroff(c.COLOR_PAIR(2));
        } else {
            _ = c.attroff(c.A_REVERSE);
        }
        return;
    };
    defer fields.deinit();

    var lines = std.ArrayList([]const u8){};
    defer lines.deinit(temp_allocator);
    buildRowPanelLines(&lines, preview, fields.items, selected_row, selected_col, temp_allocator) catch {
        if (c.has_colors()) {
            _ = c.attroff(c.COLOR_PAIR(2));
        } else {
            _ = c.attroff(c.A_REVERSE);
        }
        return;
    };

    var row_i: usize = 0;
    while (row_i < content_h) : (row_i += 1) {
        const cy = panel_y + 1 + @as(c_int, @intCast(row_i));
        const cx = panel_x + 2;
        if (scroll_row + row_i < lines.items.len) {
            const raw = lines.items[scroll_row + row_i];
            const line = if (scroll_col < raw.len) raw[scroll_col..] else "";
            const take = @min(content_w, line.len);
            if (take > 0) {
                _ = c.mvaddnstr(cy, cx, line.ptr, @intCast(take));
            }
            if (take < content_w) {
                _ = c.mvhline(cy, cx + @as(c_int, @intCast(take)), ' ', @intCast(content_w - take));
            }
        } else {
            _ = c.mvhline(cy, cx, ' ', @intCast(content_w));
        }
    }

    if (c.has_colors()) {
        _ = c.attroff(c.COLOR_PAIR(2));
    } else {
        _ = c.attroff(c.A_REVERSE);
    }
}

fn buildRowPanelLines(
    lines: *std.ArrayList([]const u8),
    preview: *pv.PreviewData,
    values: []const []const u8,
    selected_row: usize,
    selected_col: usize,
    allocator: std.mem.Allocator,
) !void {
    const col_name = if (selected_col < preview.headers.len) preview.headers[selected_col] else "<none>";
    const cell_value = if (selected_col < values.len) values[selected_col] else "";
    try lines.append(allocator, try std.fmt.allocPrint(allocator, "row_index: {d}", .{selected_row}));
    try lines.append(allocator, try std.fmt.allocPrint(allocator, "selected_column: {s}", .{col_name}));
    try lines.append(allocator, try std.fmt.allocPrint(allocator, "selected_value: {s}", .{cell_value}));
    try lines.append(allocator, try allocator.dupe(u8, "{"));

    const headers = preview.headers;
    const limit = @min(headers.len, values.len);
    for (0..limit) |i| {
        const suffix = if (i + 1 < limit) "," else "";
        try lines.append(
            allocator,
            try std.fmt.allocPrint(allocator, "  \"{s}\": \"{s}\"{s}", .{ headers[i], values[i], suffix }),
        );
    }
    try lines.append(allocator, try allocator.dupe(u8, "}"));
}

fn visibleBodyRows() usize {
    const h = c.getmaxy(c.stdscr);
    const available = h - 5;
    return @intCast(if (available > 0) available else 0);
}

/// Sync body lines loaded before the first frame so the grid is populated immediately; background appends the rest.
fn initialSyncBodyLines() usize {
    const v = visibleBodyRows();
    return @max(128, v * 4);
}

fn visibleColumnCount() usize {
    const w = c.getmaxx(c.stdscr);
    const available = w - row_marker_width;
    return @intCast(if (available > 0) @divFloor(available, col_width) else 0);
}

