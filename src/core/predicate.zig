const std = @import("std");

pub const Operator = enum {
    eq,
    neq,
    gt,
    lt,
    contains,
    in_list,
};

pub const Condition = struct {
    column: []const u8,
    op: Operator,
    value: []const u8,
};

pub const ResolvedCondition = struct {
    index: usize,
    op: Operator,
    value: []const u8,
};

pub fn eq(lhs: []const u8, rhs: []const u8) bool {
    return std.mem.eql(u8, lhs, rhs);
}

pub fn parseConditions(allocator: std.mem.Allocator, text: []const u8) ![]Condition {
    var out: std.ArrayList(Condition) = .empty;
    defer out.deinit(allocator);

    var parts = std.mem.splitScalar(u8, text, ',');
    while (parts.next()) |part| {
        if (part.len == 0) continue;
        const p = std.mem.trim(u8, part, " ");
        if (p.len == 0) continue;

        if (std.mem.indexOf(u8, p, " contains ")) |sep| {
            const col = std.mem.trim(u8, p[0..sep], " ");
            const val = std.mem.trim(u8, p[sep + " contains ".len ..], " ");
            if (col.len == 0) return error.InvalidFilterExpression;
            try out.append(allocator, .{ .column = col, .op = .contains, .value = val });
            continue;
        }
        if (std.mem.indexOf(u8, p, " in ")) |sep| {
            const col = std.mem.trim(u8, p[0..sep], " ");
            const val = std.mem.trim(u8, p[sep + " in ".len ..], " ");
            if (col.len == 0) return error.InvalidFilterExpression;
            try out.append(allocator, .{ .column = col, .op = .in_list, .value = val });
            continue;
        }
        if (std.mem.indexOf(u8, p, "!=")) |sep| {
            const col = std.mem.trim(u8, p[0..sep], " ");
            const val = std.mem.trim(u8, p[sep + 2 ..], " ");
            if (col.len == 0) return error.InvalidFilterExpression;
            try out.append(allocator, .{ .column = col, .op = .neq, .value = val });
            continue;
        }
        if (std.mem.indexOfScalar(u8, p, '>')) |sep| {
            const col = std.mem.trim(u8, p[0..sep], " ");
            const val = std.mem.trim(u8, p[sep + 1 ..], " ");
            if (col.len == 0) return error.InvalidFilterExpression;
            try out.append(allocator, .{ .column = col, .op = .gt, .value = val });
            continue;
        }
        if (std.mem.indexOfScalar(u8, p, '<')) |sep| {
            const col = std.mem.trim(u8, p[0..sep], " ");
            const val = std.mem.trim(u8, p[sep + 1 ..], " ");
            if (col.len == 0) return error.InvalidFilterExpression;
            try out.append(allocator, .{ .column = col, .op = .lt, .value = val });
            continue;
        }
        if (std.mem.indexOfScalar(u8, p, '=')) |sep| {
            const col = std.mem.trim(u8, p[0..sep], " ");
            const val = std.mem.trim(u8, p[sep + 1 ..], " ");
            if (col.len == 0) return error.InvalidFilterExpression;
            try out.append(allocator, .{ .column = col, .op = .eq, .value = val });
            continue;
        }

        return error.InvalidFilterExpression;
    }
    return out.toOwnedSlice(allocator);
}

pub fn rowMatchesAll(fields: []const []const u8, conditions: []const ResolvedCondition) bool {
    for (conditions) |cond| {
        if (cond.index >= fields.len) return false;
        if (!matchField(fields[cond.index], cond)) return false;
    }
    return true;
}

fn matchField(field: []const u8, cond: ResolvedCondition) bool {
    return switch (cond.op) {
        .eq => eq(field, cond.value),
        .neq => !eq(field, cond.value),
        .contains => std.mem.indexOf(u8, field, cond.value) != null,
        .gt => compareNumber(field, cond.value, .gt),
        .lt => compareNumber(field, cond.value, .lt),
        .in_list => inList(field, cond.value),
    };
}

const Cmp = enum { gt, lt };

fn compareNumber(field: []const u8, rhs_text: []const u8, cmp: Cmp) bool {
    const lhs = std.fmt.parseFloat(f64, field) catch return false;
    const rhs = std.fmt.parseFloat(f64, rhs_text) catch return false;
    return switch (cmp) {
        .gt => lhs > rhs,
        .lt => lhs < rhs,
    };
}

fn inList(field: []const u8, raw_list: []const u8) bool {
    var items = std.mem.splitScalar(u8, raw_list, '|');
    while (items.next()) |item| {
        if (eq(field, std.mem.trim(u8, item, " "))) return true;
    }
    return false;
}
