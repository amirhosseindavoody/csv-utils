const std = @import("std");
const schema = @import("../core/schema.zig");
const c = @cImport({
    @cInclude("ncurses.h");
});

const detail_panel_height: c_int = 5;
const row_marker_width: c_int = 2;
const col_width: c_int = 16;

const PreviewData = struct {
    allocator: std.mem.Allocator,
    headers: [][]u8,
    rows: [][]u8,

    fn deinit(self: *PreviewData) void {
        for (self.headers) |h| self.allocator.free(h);
        for (self.rows) |r| self.allocator.free(r);
        self.allocator.free(self.headers);
        self.allocator.free(self.rows);
    }
};

const ScanState = struct {
    mutex: std.Thread.Mutex = .{},
    rows_seen: usize = 0,
    bytes_seen: usize = 0,
    done: bool = false,
    has_error: bool = false,
};

pub fn run(file_path: ?[]const u8) !void {
    var preview = try loadPreview(std.heap.page_allocator, file_path, 500);
    defer preview.deinit();

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
    }

    var state = ScanState{};
    var scan_thread: ?std.Thread = null;
    if (file_path) |path| {
        scan_thread = try std.Thread.spawn(.{}, backgroundScanLoop, .{ path, &state });
    }
    defer if (scan_thread) |t| t.join();

    var selected_row: usize = 0;
    var selected_col: usize = 0;
    var row_offset: usize = 0;
    var col_offset: usize = 0;
    var running = true;
    while (running) {
        var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
        defer arena.deinit();
        drawUi(file_path, &preview, selected_row, selected_col, row_offset, col_offset, &state, arena.allocator());
        const ch = c.getch();

        const max_rows: usize = if (preview.rows.len == 0) 0 else preview.rows.len - 1;
        const max_cols: usize = if (preview.headers.len == 0) 0 else preview.headers.len - 1;
        switch (ch) {
            'q' => running = false,
            c.KEY_UP => {
                if (selected_row > 0) selected_row -= 1;
            },
            c.KEY_DOWN => {
                if (selected_row < max_rows) selected_row += 1;
            },
            c.KEY_LEFT => {
                if (selected_col > 0) selected_col -= 1;
            },
            c.KEY_RIGHT => {
                if (selected_col < max_cols) selected_col += 1;
            },
            c.KEY_NPAGE => {
                const page = visibleBodyRows();
                selected_row = @min(max_rows, selected_row + page);
            },
            c.KEY_PPAGE => {
                const page = visibleBodyRows();
                selected_row = selected_row -| page;
            },
            c.KEY_HOME => {
                selected_row = 0;
                selected_col = 0;
            },
            c.KEY_END => {
                selected_row = max_rows;
            },
            else => {},
        }

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

fn drawUi(
    file_path: ?[]const u8,
    preview: *const PreviewData,
    selected_row: usize,
    selected_col: usize,
    row_offset: usize,
    col_offset: usize,
    state: *ScanState,
    temp_allocator: std.mem.Allocator,
) void {
    _ = c.erase();

    const h = c.getmaxy(c.stdscr);
    const w = c.getmaxx(c.stdscr);

    _ = c.mvaddstr(0, 0, "csv-utils TUI (q quit, arrows navigate)");

    var buffer: [256]u8 = undefined;
    state.mutex.lock();
    const rows = state.rows_seen;
    const done = state.done;
    const has_error = state.has_error;
    state.mutex.unlock();

    const status = if (has_error)
        "error"
    else if (done)
        "complete"
    else if (file_path != null)
        "running"
    else
        "idle";
    const path = file_path orelse "<not provided>";
    const summary = std.fmt.bufPrintZ(
        &buffer,
        "file={s} scan={s} rows={d} preview={d} cell=[r{d},c{d}] col_off={d}",
        .{ path, status, rows, preview.rows.len, selected_row + 1, selected_col + 1, col_offset },
    ) catch "status unavailable";
    _ = c.mvaddstr(1, 0, summary.ptr);
    _ = c.mvhline(2, 0, '-', w);
    drawHeaderRow(3, w, preview.headers, selected_col, col_offset, temp_allocator);

    const visible_rows = visibleBodyRows();
    var screen_row: c_int = 4;
    var i: usize = 0;
    while (i < visible_rows and row_offset + i < preview.rows.len) : (i += 1) {
        const row_idx = row_offset + i;
        drawDataRow(
            screen_row,
            w,
            preview.headers.len,
            preview.rows[row_idx],
            row_idx == selected_row,
            selected_col,
            col_offset,
            temp_allocator,
        );
        screen_row += 1;
    }

    const detail_top = h - detail_panel_height;
    if (detail_top > 4) {
        _ = c.mvhline(detail_top, 0, '-', w);
        drawDetailPanel(
            detail_top + 1,
            w,
            preview,
            selected_row,
            selected_col,
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

fn drawDetailPanel(
    y: c_int,
    max_width: c_int,
    preview: *const PreviewData,
    selected_row: usize,
    selected_col: usize,
    temp_allocator: std.mem.Allocator,
) void {
    _ = c.mvaddstr(y, 0, "Selected row (JSON) / selected cell:");
    if (preview.rows.len == 0 or selected_row >= preview.rows.len) {
        _ = c.mvaddstr(y + 1, 0, "{}");
        return;
    }

    var fields = schema.splitRow(temp_allocator, preview.rows[selected_row]) catch return;
    defer fields.deinit();

    const col_name = if (selected_col < preview.headers.len) preview.headers[selected_col] else "<none>";
    const cell_value = if (selected_col < fields.items.len) fields.items[selected_col] else "";
    var selected_cell_buf: [512]u8 = undefined;
    const selected_cell_txt = std.fmt.bufPrintZ(
        &selected_cell_buf,
        "col={s} value={s}",
        .{ col_name, cell_value },
    ) catch "selected cell unavailable";
    _ = c.mvaddstr(y + 1, 0, selected_cell_txt.ptr);

    var json = std.ArrayList(u8){};
    defer json.deinit(temp_allocator);
    tryAppendJson(&json, preview.headers, fields.items, temp_allocator) catch return;

    const max_line: usize = @intCast(if (max_width > 0) max_width else 0);
    const total_lines: usize = @intCast(if (detail_panel_height > 3) detail_panel_height - 3 else 0);
    var line_idx: usize = 0;
    var offset: usize = 0;
    while (line_idx < total_lines and offset < json.items.len) : (line_idx += 1) {
        const remaining = json.items.len - offset;
        const take = @min(max_line, remaining);
        _ = c.mvaddnstr(y + 2 + @as(c_int, @intCast(line_idx)), 0, json.items[offset..].ptr, @intCast(take));
        offset += take;
    }
}

fn tryAppendJson(
    json: *std.ArrayList(u8),
    headers: [][]u8,
    values: []const []const u8,
    allocator: std.mem.Allocator,
) !void {
    try json.append(allocator, '{');
    const limit = @min(headers.len, values.len);
    for (0..limit) |i| {
        if (i != 0) try json.appendSlice(allocator, ", ");
        try json.appendSlice(allocator, "\"");
        try json.appendSlice(allocator, headers[i]);
        try json.appendSlice(allocator, "\": \"");
        try json.appendSlice(allocator, values[i]);
        try json.appendSlice(allocator, "\"");
    }
    try json.append(allocator, '}');
}

fn visibleBodyRows() usize {
    const h = c.getmaxy(c.stdscr);
    const available = h - (4 + detail_panel_height + 1);
    return @intCast(if (available > 0) available else 0);
}

fn visibleColumnCount() usize {
    const w = c.getmaxx(c.stdscr);
    const available = w - row_marker_width;
    return @intCast(if (available > 0) @divFloor(available, col_width) else 0);
}

fn loadPreview(allocator: std.mem.Allocator, file_path: ?[]const u8, limit: usize) !PreviewData {
    if (file_path == null) {
        return .{
            .allocator = allocator,
            .headers = try allocator.alloc([]u8, 0),
            .rows = try allocator.alloc([]u8, 0),
        };
    }

    var file = try std.fs.cwd().openFile(file_path.?, .{});
    defer file.close();
    var reader = file.deprecatedReader();

    const header_line = (try reader.readUntilDelimiterOrEofAlloc(allocator, '\n', 1024 * 1024)) orelse {
        return .{
            .allocator = allocator,
            .headers = try allocator.alloc([]u8, 0),
            .rows = try allocator.alloc([]u8, 0),
        };
    };
    defer allocator.free(header_line);

    var parsed_headers = try schema.splitRow(allocator, header_line);
    defer parsed_headers.deinit();

    var headers = std.ArrayList([]u8){};
    defer headers.deinit(allocator);
    for (parsed_headers.items) |h| {
        try headers.append(allocator, try allocator.dupe(u8, h));
    }

    var rows = std.ArrayList([]u8){};
    defer rows.deinit(allocator);
    while (rows.items.len < limit) {
        const line = try reader.readUntilDelimiterOrEofAlloc(allocator, '\n', 1024 * 1024);
        if (line == null) break;
        try rows.append(allocator, line.?);
    }

    return .{
        .allocator = allocator,
        .headers = try headers.toOwnedSlice(allocator),
        .rows = try rows.toOwnedSlice(allocator),
    };
}

fn backgroundScanLoop(file_path: []const u8, state: *ScanState) void {
    var file = std.fs.cwd().openFile(file_path, .{}) catch {
        state.mutex.lock();
        state.has_error = true;
        state.done = true;
        state.mutex.unlock();
        return;
    };
    defer file.close();

    var reader = file.deprecatedReader();
    while (true) {
        const line = reader.readUntilDelimiterOrEofAlloc(std.heap.page_allocator, '\n', 1024 * 1024) catch {
            state.mutex.lock();
            state.has_error = true;
            state.done = true;
            state.mutex.unlock();
            return;
        };
        if (line == null) break;
        defer std.heap.page_allocator.free(line.?);

        state.mutex.lock();
        state.rows_seen += 1;
        state.bytes_seen += line.?.len;
        state.mutex.unlock();
    }

    state.mutex.lock();
    state.done = true;
    state.mutex.unlock();
}
