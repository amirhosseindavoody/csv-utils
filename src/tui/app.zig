const std = @import("std");
const c = @cImport({
    @cInclude("ncurses.h");
});

const ScanState = struct {
    mutex: std.Thread.Mutex = .{},
    rows_seen: usize = 0,
    bytes_seen: usize = 0,
    done: bool = false,
    has_error: bool = false,
};

pub fn run(file_path: ?[]const u8) !void {
    _ = c.initscr();
    defer _ = c.endwin();
    _ = c.cbreak();
    _ = c.noecho();
    _ = c.keypad(c.stdscr, true);
    _ = c.nodelay(c.stdscr, true);
    _ = c.curs_set(0);
    _ = c.timeout(100);

    var state = ScanState{};
    var scan_thread: ?std.Thread = null;
    if (file_path) |path| {
        scan_thread = try std.Thread.spawn(.{}, backgroundScanLoop, .{ path, &state });
    }
    defer if (scan_thread) |t| t.join();

    var selected_pane: usize = 0;
    var running = true;
    while (running) {
        drawUi(file_path, selected_pane, &state);
        const ch = c.getch();
        switch (ch) {
            'q' => running = false,
            '\t' => selected_pane = (selected_pane + 1) % 3,
            else => {},
        }
    }
}

fn drawUi(file_path: ?[]const u8, selected_pane: usize, state: *ScanState) void {
    _ = c.erase();

    const h = c.getmaxy(c.stdscr);
    const w = c.getmaxx(c.stdscr);
    const mid = @divFloor(w, 2);

    _ = c.mvaddstr(0, 0, "csv-utils TUI (q=quit, tab=switch pane)");
    _ = c.mvaddstr(1, 0, "--------------------------------------");

    drawBox(2, 0, h - 2, mid, selected_pane == 0, "Overview");
    drawBox(2, mid, h - 2, w - mid, selected_pane == 1, "Background Scan");

    var buffer: [256]u8 = undefined;
    if (file_path) |path| {
        const txt = std.fmt.bufPrintZ(&buffer, "File: {s}", .{path}) catch "File: <format-error>";
        _ = c.mvaddstr(4, 2, txt.ptr);
    } else {
        _ = c.mvaddstr(4, 2, "File: <not provided>");
    }

    state.mutex.lock();
    const rows = state.rows_seen;
    const bytes = state.bytes_seen;
    const done = state.done;
    const has_error = state.has_error;
    state.mutex.unlock();

    const rows_txt = std.fmt.bufPrintZ(&buffer, "Rows seen: {d}", .{rows}) catch "Rows seen: ?";
    _ = c.mvaddstr(5, 2, rows_txt.ptr);
    const bytes_txt = std.fmt.bufPrintZ(&buffer, "Bytes seen: {d}", .{bytes}) catch "Bytes seen: ?";
    _ = c.mvaddstr(6, 2, bytes_txt.ptr);

    if (has_error) {
        _ = c.mvaddstr(4, mid + 2, "Scan status: error");
    } else if (done) {
        _ = c.mvaddstr(4, mid + 2, "Scan status: complete");
    } else if (file_path != null) {
        _ = c.mvaddstr(4, mid + 2, "Scan status: running");
    } else {
        _ = c.mvaddstr(4, mid + 2, "Scan status: idle");
    }
    _ = c.mvaddstr(6, mid + 2, "Pane 1: overview");
    _ = c.mvaddstr(7, mid + 2, "Pane 2: scan progress");
    _ = c.mvaddstr(8, mid + 2, "Pane 3: reserved for filters");

    _ = c.refresh();
}

fn drawBox(y: c_int, x: c_int, height: c_int, width: c_int, selected: bool, title: [*:0]const u8) void {
    var i: c_int = 0;
    while (i < width) : (i += 1) {
        _ = c.mvaddch(y, x + i, if (selected) '=' else '-');
        _ = c.mvaddch(y + height - 1, x + i, if (selected) '=' else '-');
    }
    i = 0;
    while (i < height) : (i += 1) {
        _ = c.mvaddch(y + i, x, if (selected) '|' else ':');
        _ = c.mvaddch(y + i, x + width - 1, if (selected) '|' else ':');
    }
    _ = c.mvaddstr(y, x + 2, title);
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
