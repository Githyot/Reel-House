-- Reelhouse: adds a "Quality" button to the mpv window.
-- The app sends the list of available streams with:  script-message rh-set-qualities <json>
-- Picking an entry sends back:                       script-message rh-quality-pick <index>
-- The button appears when the mouse moves. You can also press Y to open the menu.

local mp = require 'mp'
local utils = require 'mp.utils'
local assdraw = require 'mp.assdraw'

local qualities = {}
local overlay = mp.create_osd_overlay("ass-events")
local menu_open = false
local selected = 1
local last_move = -1000
local hover_button = false
local hide_timer = nil

local function scale()
    local _, h = mp.get_osd_size()
    if not h or h <= 0 then return 1 end
    return h / 720
end

local function geometry()
    local w, h = mp.get_osd_size()
    local s = scale()
    local g = { w = w, h = h, s = s }
    g.bw, g.bh = 150 * s, 40 * s
    g.bx, g.by = w - g.bw - 24 * s, 24 * s
    g.pw = 400 * s
    g.px, g.py = w - g.pw - 24 * s, 24 * s
    g.row = 52 * s
    g.head = 52 * s
    return g
end

local function rect(ass, x0, y0, x1, y1, color, alpha)
    ass:new_event()
    ass:pos(0, 0)
    ass:append(string.format("{\\1c&H%s&\\1a&H%s&\\bord0\\shad0}", color, alpha))
    ass:draw_start()
    ass:rect_cw(x0, y0, x1, y1)
    ass:draw_stop()
end

local function text(ass, x, y, an, size, color, str)
    ass:new_event()
    ass:pos(x, y)
    ass:append(string.format("{\\an%d\\fs%d\\1c&H%s&\\bord0\\shad0}%s", an, math.floor(size), color, str))
end

local function button_visible()
    return menu_open or (mp.get_time() - last_move) < 3 or mp.get_property_bool("pause", false)
end

local function redraw()
    local g = geometry()
    if not g.w or g.w <= 0 then return end
    overlay.res_x, overlay.res_y = g.w, g.h
    local ass = assdraw.ass_new()
    if menu_open then
        local height = g.head + #qualities * g.row + 12 * g.s
        rect(ass, g.px, g.py, g.px + g.pw, g.py + height, "1A1411", "20")
        text(ass, g.px + 20 * g.s, g.py + 26 * g.s, 4, 22 * g.s, "FFFFFF", "{\\b1}Quality")
        text(ass, g.px + g.pw - 16 * g.s, g.py + 26 * g.s, 6, 14 * g.s, "B5A99A", "Esc to close")
        for i, q in ipairs(qualities) do
            local y = g.py + g.head + (i - 1) * g.row
            if i == selected then
                rect(ass, g.px + 8 * g.s, y, g.px + g.pw - 8 * g.s, y + g.row - 4 * g.s, "3A2A5E", "40")
            end
            local mark = q.current and "\\N" or ""
            local label = (q.current and "\xE2\x9C\x93 " or "") .. (q.label or "")
            text(ass, g.px + 20 * g.s, y + 16 * g.s, 4, 19 * g.s, q.current and "4D6AFF" or "FFFFFF", "{\\b1}" .. label)
            text(ass, g.px + 20 * g.s, y + 38 * g.s, 4, 14 * g.s, "B5A99A", q.detail or "")
        end
    elseif #qualities > 0 and button_visible() then
        rect(ass, g.bx, g.by, g.bx + g.bw, g.by + g.bh, hover_button and "4D6AFF" or "1A1411", "30")
        text(ass, g.bx + g.bw / 2, g.by + g.bh / 2, 5, 18 * g.s, "FFFFFF", "{\\b1}Quality  [Y]")
    end
    overlay.data = ass.text
    overlay:update()
end

local function in_box(x, y, bx, by, bw, bh)
    return x >= bx and x <= bx + bw and y >= by and y <= by + bh
end

local function row_at(x, y)
    local g = geometry()
    if not in_box(x, y, g.px, g.py, g.pw, g.head + #qualities * g.row) then return nil, true end
    local i = math.floor((y - g.py - g.head) / g.row) + 1
    if i >= 1 and i <= #qualities then return i, false end
    return nil, false
end

local close_menu

local function pick(i)
    local q = qualities[i]
    if not q then return end
    close_menu()
    if q.current then return end
    mp.osd_message("Switching to " .. (q.label or "stream") .. "...", 6)
    mp.commandv("script-message", "rh-quality-pick", tostring(q.index))
end

local function menu_click()
    local x, y = mp.get_mouse_pos()
    local i, outside = row_at(x, y)
    if i then
        pick(i)
    elseif outside then
        close_menu()
    end
end

local function open_menu()
    if #qualities == 0 then
        mp.osd_message("No other qualities found for this title.", 3)
        return
    end
    menu_open = true
    selected = 1
    for i, q in ipairs(qualities) do if q.current then selected = i end end
    mp.add_forced_key_binding("UP", "rh-up", function() selected = math.max(1, selected - 1); redraw() end, { repeatable = true })
    mp.add_forced_key_binding("DOWN", "rh-down", function() selected = math.min(#qualities, selected + 1); redraw() end, { repeatable = true })
    mp.add_forced_key_binding("ENTER", "rh-enter", function() pick(selected) end)
    mp.add_forced_key_binding("ESC", "rh-esc", function() close_menu() end)
    mp.add_forced_key_binding("MBTN_LEFT", "rh-menu-click", menu_click)
    redraw()
end

close_menu = function()
    menu_open = false
    for _, name in ipairs({ "rh-up", "rh-down", "rh-enter", "rh-esc", "rh-menu-click" }) do
        mp.remove_key_binding(name)
    end
    redraw()
end

local function schedule_hide()
    if hide_timer then hide_timer:kill() end
    hide_timer = mp.add_timeout(3.2, function() redraw() end)
end

-- The click binding exists only while the pointer is over the button, so normal clicks keep working.
local button_bound = false
local function update_button_binding(x, y)
    local g = geometry()
    local over = (not menu_open) and #qualities > 0 and button_visible() and in_box(x, y, g.bx, g.by, g.bw, g.bh)
    hover_button = over
    if over and not button_bound then
        mp.add_forced_key_binding("MBTN_LEFT", "rh-button-click", open_menu)
        button_bound = true
    elseif not over and button_bound then
        mp.remove_key_binding("rh-button-click")
        button_bound = false
    end
end

mp.observe_property("mouse-pos", "native", function(_, pos)
    if not pos then return end
    last_move = mp.get_time()
    update_button_binding(pos.x or 0, pos.y or 0)
    redraw()
    schedule_hide()
end)

mp.observe_property("osd-dimensions", "native", function() redraw() end)
mp.observe_property("pause", "bool", function() redraw() end)

mp.add_key_binding("y", "rh-quality-key", function()
    if menu_open then close_menu() else open_menu() end
end)

mp.register_script_message("rh-set-qualities", function(json)
    local list = utils.parse_json(json)
    qualities = type(list) == "table" and list or {}
    redraw()
end)
