#!/usr/bin/env python3
"""Writes the Hearth watch face (Watch Face Format 4): res/raw/watchface.xml and res/values/strings.xml.

Run it after a change: python3 watchface/gen.py

How the theme gets onto the face: a face can't read the app, but it can read a complication. Slot 0 (full screen, not
editable) is fed by Hearth's FaceComplication: the 5-hour usage as its value, the status line as its title, and the
theme chosen in the watch app as four colours (accent, text, quiet text, ring track). Everything that follows the theme
is drawn inside that slot and picks its colour with extractColorFromColors. Without Hearth (another source, or none),
the same face is drawn with the colours picked in the face editor.

Battery: black background, no seconds and no glow by default; the always-on screen shows only a thin time.
"""
import os

HERE = os.path.dirname(os.path.abspath(__file__))
RES = os.path.join(HERE, "src/main/res")

COLORS = "[COMPLICATION.RANGED_VALUE_COLORS]"
PICK = {"accent": "0.05", "ink": "0.4", "muted": "0.7", "track": "1.0"}   # the four theme colours, see FaceComplication
FRAC = {
    "claude": "(clamp([COMPLICATION.RANGED_VALUE_VALUE], [COMPLICATION.RANGED_VALUE_MIN], [COMPLICATION.RANGED_VALUE_MAX]) - [COMPLICATION.RANGED_VALUE_MIN]) / ([COMPLICATION.RANGED_VALUE_MAX] - [COMPLICATION.RANGED_VALUE_MIN])",
    "battery": "clamp([BATTERY_PERCENT], 0, 100) / 100",
    "steps": "clamp([STEP_PERCENT], 0, 100) / 100",
}
ACCENTS = [("clay", "Hearth clay", "#F0643C"), ("ocean", "Ocean blue", "#5AA9F5"), ("forest", "Forest green", "#6FCF7F"),
           ("grape", "Grape", "#B98CF0"), ("mono", "Mono", "#E6E6E6"), ("rose", "Rose", "#F27BA0"), ("amber", "Amber", "#F2B33D")]
FIXED = {"ink": "#FFF5F1EA", "muted": "#FFA8A29A", "track": "#FF262320"}
TIME_STYLES = [("large", "Large", 124, "NORMAL"), ("thin", "Large thin", 124, "THIN"), ("bold", "Large bold", 118, "BOLD"),
               ("medium", "Medium", 100, "NORMAL"), ("mbold", "Medium bold", 96, "BOLD")]
TIME_COLORS = [("white", "White"), ("warm", "Warm white"), ("accent", "Theme colour"), ("text", "Theme text")]
DATE_STYLES = [("short", "Thu 19", "%s %s", ["[DAY_OF_WEEK_S]", "[DAY]"]), ("month", "19 Nov", "%s %s", ["[DAY]", "[MONTH_S]"]),
               ("full", "Thursday 19", "%s %s", ["[DAY_OF_WEEK_F]", "[DAY]"])]
RING_DATA = [("claude", "Claude usage (5h)"), ("battery", "Battery"), ("steps", "Steps goal"), ("off", "Off")]
RING_WIDTH = [("thin", "Thin", 8), ("medium", "Medium", 14), ("bold", "Bold", 22)]
BACKGROUNDS = [("black", "Black", 0), ("tint", "Soft colour", 30), ("deep", "Deep colour", 60)]

strings: dict[str, str] = {}
def s(key: str, text: str) -> str: strings[key] = text; return key


def grp(inner: str, name: str = "part") -> str:
    """Options can't hold another option list directly: it goes in a Group (empty when the option shows nothing)."""
    if not inner.strip(): return f'<Group x="0" y="0" width="450" height="450" name="{name}"/>'
    return f'<Group x="0" y="0" width="450" height="450" name="{name}">\n{ind(inner, 2)}\n</Group>'


def ind(t: str, n: int) -> str: return "\n".join((" " * n + l) if l.strip() else l for l in t.split("\n"))


def color_attr(kind: str, themed: bool) -> str:
    """The colour of an element: in the themed face a placeholder plus a Transform (added by tr()), else the editor's colour."""
    if kind == "accent": return "[CONFIGURATION.accent.0]"
    return FIXED[kind]


def tr(kind: str, themed: bool) -> str:
    return f'<Transform target="color" value="extractColorFromColors({COLORS}, false, {PICK[kind]})"/>' if themed else ""


def text(x: int, y: int, w: int, h: int, size: int, kind: str, themed: bool, template: str, params: list[str], weight: str = "NORMAL", align: str = "CENTER", ambient_hide: bool = True) -> str:
    p = "".join(f'<Parameter expression="{e}"/>' for e in params)
    v = '\n  <Variant mode="AMBIENT" target="alpha" value="0"/>' if ambient_hide else ""
    return f'''<PartText x="{x}" y="{y}" width="{w}" height="{h}">{v}
  <Text align="{align}" ellipsis="TRUE">
    <Font family="SYNC_TO_DEVICE" size="{size}" weight="{weight}" slant="NORMAL" color="{color_attr(kind, themed)}">
      {tr(kind, themed)}<Template>{template}{p}</Template>
    </Font>
  </Text>
</PartText>'''


def arc(cx: float, cy: float, d: float, a0: int, a1: int, width: int, kind: str, themed: bool, frac: str | None = None, ambient: int = 255) -> str:
    t = f'\n    <Transform target="endAngle" value="{a0} + ({a1} - ({a0})) * ({frac})"/>' if frac else ""
    return f'''<PartDraw x="0" y="0" width="450" height="450">
  <Variant mode="AMBIENT" target="alpha" value="{ambient}"/>
  <Arc centerX="{cx}" centerY="{cy}" width="{d}" height="{d}" startAngle="{a0}" endAngle="{a1}">
    <Stroke thickness="{width}" color="{color_attr(kind, themed)}" cap="ROUND">{tr(kind, themed)}</Stroke>{t}
  </Arc>
</PartDraw>'''


def ring(themed: bool, has_value: bool) -> str:
    """The ring along the edge: what it shows, how thick, and whether it stays (dimmed) on the always-on screen."""
    opts = []
    for i, (key, _) in enumerate(RING_DATA):
        if key == "off" or (key == "claude" and not has_value):
            opts.append(f'<ListOption id="{i}">\n{ind(grp("" if key == "off" else ring_widths(themed, None), "ring"), 2)}\n</ListOption>')
            continue
        opts.append(f'<ListOption id="{i}">\n{ind(grp(ring_widths(themed, FRAC[key]), "ring"), 2)}\n</ListOption>')
    return '<ListConfiguration id="ringData">\n' + ind("\n".join(opts), 2) + "\n</ListConfiguration>"


def ring_widths(themed: bool, frac: str | None) -> str:
    out = []
    for i, (_, _, w) in enumerate(RING_WIDTH):
        d = 450 - w - 8
        parts = []
        for keep in ("TRUE", "FALSE"):
            amb = 90 if keep == "TRUE" else 0
            track = arc(225, 225, d, -135, 135, w, "track", themed, ambient=0)
            prog = arc(225, 225, d, -135, 135, w, "accent", themed, frac, ambient=amb) if frac else ""
            parts.append(f'<BooleanOption id="{keep}">\n{ind(grp(track + chr(10) + prog, "arcs"), 2)}\n</BooleanOption>')
        inner = '<BooleanConfiguration id="ambientRing">\n' + ind("\n".join(parts), 2) + "\n</BooleanConfiguration>"
        out.append(f'<ListOption id="{i}">\n{ind(grp(inner, "width"), 2)}\n</ListOption>')
    return '<ListConfiguration id="ringWidth">\n' + ind("\n".join(out), 2) + "\n</ListConfiguration>"


def background(themed: bool) -> str:
    out = []
    for i, (_, _, a) in enumerate(BACKGROUNDS):
        if a == 0: out.append(f'<ListOption id="{i}">\n{ind(grp("", "bg"), 2)}\n</ListOption>'); continue
        out.append(f'''<ListOption id="{i}">
  <PartDraw x="0" y="0" width="450" height="450" alpha="{a}">
    <Variant mode="AMBIENT" target="alpha" value="0"/>
    <Ellipse x="0" y="0" width="450" height="450">
      <Fill color="{color_attr("accent", themed)}">{tr("accent", themed)}</Fill>
    </Ellipse>
  </PartDraw>
</ListOption>''')
    return '<ListConfiguration id="background">\n' + ind("\n".join(out), 2) + "\n</ListConfiguration>"


def ticks(themed: bool) -> str:
    import math
    lines = []
    for h in range(12):
        a = math.radians(h * 30 - 90)
        r0, r1 = (176, 192) if h % 3 == 0 else (182, 192)
        lines.append(f'<Line startX="{225 + r0 * math.cos(a):.1f}" startY="{225 + r0 * math.sin(a):.1f}" endX="{225 + r1 * math.cos(a):.1f}" endY="{225 + r1 * math.sin(a):.1f}">\n'
                     f'  <Stroke thickness="{4 if h % 3 == 0 else 2}" color="{color_attr("muted", themed)}" cap="ROUND">{tr("muted", themed)}</Stroke>\n</Line>')
    body = ind("\n".join(lines), 4)
    return f'''<BooleanConfiguration id="ticks">
  <BooleanOption id="TRUE">
    <PartDraw x="0" y="0" width="450" height="450">
      <Variant mode="AMBIENT" target="alpha" value="0"/>
{ind(body, 2)}
    </PartDraw>
  </BooleanOption>
</BooleanConfiguration>'''


def time_block(themed: bool) -> str:
    """The time. White and warm white use the watch's own clock; the theme colours draw it as text that follows the theme."""
    out = []
    for si, (_, _, size, weight) in enumerate(TIME_STYLES):
        y = 128 if size > 110 else 140
        h = size + 20
        cols = []
        for ci, (ck, _) in enumerate(TIME_COLORS):
            if ck in ("white", "warm"):
                col = "#FFFFFFFF" if ck == "white" else FIXED["ink"]
                body = f'''<DigitalClock x="0" y="0" width="450" height="450">
  <Variant mode="AMBIENT" target="alpha" value="0"/>
  <TimeText format="hh:mm" hourFormat="SYNC_TO_DEVICE" align="CENTER" x="25" y="{y}" width="400" height="{h}">
    <Font family="SYNC_TO_DEVICE" size="{size}" weight="{weight}" slant="NORMAL" color="{col}"/>
  </TimeText>
</DigitalClock>'''
            else:
                kind = "accent" if ck == "accent" else "ink"
                parts = []
                for h24, hour in (("TRUE", "[HOUR_0_23_Z]"), ("FALSE", "[HOUR_1_12]")):
                    parts.append((h24, text(25, y, 400, h, size, kind, themed, "%s:%s", [hour, "[MINUTE_Z]"], weight)))
                body = f'''<Condition>
  <Expressions><Expression name="h24"><![CDATA[[IS_24_HOUR_MODE] == 1]]></Expression></Expressions>
  <Compare expression="h24">
{ind(parts[0][1], 4)}
  </Compare>
  <Default>
{ind(parts[1][1], 4)}
  </Default>
</Condition>'''
            cols.append(f'<ListOption id="{ci}">\n{ind(body, 2)}\n</ListOption>')
        inner = '<ListConfiguration id="timeColor">\n' + ind("\n".join(cols), 2) + "\n</ListConfiguration>"
        out.append(f'<ListOption id="{si}">\n{ind(grp(inner, "time"), 2)}\n</ListOption>')
    return '<ListConfiguration id="timeStyle">\n' + ind("\n".join(out), 2) + "\n</ListConfiguration>"


def date_block(themed: bool) -> str:
    opts = [f'<ListOption id="{i}">\n{ind(text(75, 76, 300, 44, 30, "accent", themed, tpl, ps), 2)}\n</ListOption>' for i, (_, _, tpl, ps) in enumerate(DATE_STYLES)]
    return f'''<BooleanConfiguration id="showDate">
  <BooleanOption id="TRUE">
{ind(grp('<ListConfiguration id="dateStyle">' + chr(10) + ind(chr(10).join(opts), 2) + chr(10) + '</ListConfiguration>', "date"), 4)}
  </BooleanOption>
</BooleanConfiguration>'''


def status_block(themed: bool) -> str:
    return f'''<BooleanConfiguration id="showStatus">
  <BooleanOption id="TRUE">
{ind(text(90, 268, 270, 36, 24, "muted", themed, "%s", ["[COMPLICATION.TITLE]"]), 4)}
  </BooleanOption>
</BooleanConfiguration>'''


def face(themed: bool, has_value: bool) -> str:
    parts = [background(themed), ticks(themed), ring(themed, has_value), date_block(themed), time_block(themed)]
    if has_value: parts.append(status_block(themed))
    return '<Group x="0" y="0" width="450" height="450" name="face">\n' + ind("\n".join(parts), 2) + "\n</Group>"


def hearth_slot() -> str:
    themed = f'''<Condition>
  <Expressions><Expression name="themed"><![CDATA[{COLORS} != null]]></Expression></Expressions>
  <Compare expression="themed">
{ind(face(True, True), 4)}
  </Compare>
  <Default>
{ind(face(False, True), 4)}
  </Default>
</Condition>'''
    return f'''<!-- slot 0: the face itself, in the colours of the Hearth theme (see the top of gen.py) -->
<ComplicationSlot x="0" y="0" width="450" height="450" slotId="0" displayName="{s("slot_hearth", "Hearth")}" supportedTypes="RANGED_VALUE EMPTY" isCustomizable="FALSE">
  <DefaultProviderPolicy defaultSystemProvider="EMPTY" defaultSystemProviderType="EMPTY" primaryProvider="dev.voer.hearth/dev.voer.hearth.wear.FaceComplication" primaryProviderType="RANGED_VALUE"/>
  <BoundingBox x="90" y="120" width="270" height="150"/>
  <Complication type="RANGED_VALUE">
{ind(themed, 4)}
  </Complication>
  <Complication type="EMPTY">
{ind(face(False, False), 4)}
  </Complication>
</ComplicationSlot>'''


def sub_slot(slot: int, x: int, y: int, w: int, h: int, name: str, label: str, types: list[str], default: tuple[str, str], primary: tuple[str, str] | None = None) -> str:
    """A spot for any complication. Hearth's own come with the theme colour; others take the face's accent."""
    pp = f' primaryProvider="dev.voer.hearth/dev.voer.hearth.wear.{primary[0]}" primaryProviderType="{primary[1]}"' if primary else ""
    big = h >= 70
    rv_ramp = "[COMPLICATION.RANGED_VALUE_COLORS]"
    cx, cy = w // 2, h // 2
    d = min(w, h) - 10
    comps = []
    if "SHORT_TEXT" in types:
        icon_text = f'''<Condition>
  <Expressions><Expression name="icon"><![CDATA[[COMPLICATION.MONOCHROMATIC_IMAGE] != null]]></Expression></Expressions>
  <Compare expression="icon">
    <PartImage x="{cx - 14}" y="{4 if big else 6}" width="28" height="28" tintColor="[CONFIGURATION.accent.0]">
      <Image resource="[COMPLICATION.MONOCHROMATIC_IMAGE]"/>
    </PartImage>
{ind(plain_text(0, 34 if big else 8, w, 40 if big else 30, 34 if big else 26, "#FFF5F1EA", "[COMPLICATION.TEXT]"), 4) if big else ind(plain_text(30, 6, w - 30, 30, 26, "#FFF5F1EA", "[COMPLICATION.TEXT]"), 4)}
  </Compare>
  <Default>
{ind(plain_text(0, 6 if big else 4, w, 44 if big else 34, 38 if big else 28, "#FFF5F1EA", "[COMPLICATION.TEXT]"), 4)}
{ind(plain_text(0, 52, w, 28, 22, "#FFA8A29A", "[COMPLICATION.TITLE]"), 4) if big else ""}
  </Default>
</Condition>'''
        comps.append(f'<Complication type="SHORT_TEXT">\n{ind(icon_text, 2)}\n</Complication>')
    if "RANGED_VALUE" in types:
        frac = "(clamp([COMPLICATION.RANGED_VALUE_VALUE], [COMPLICATION.RANGED_VALUE_MIN], [COMPLICATION.RANGED_VALUE_MAX]) - [COMPLICATION.RANGED_VALUE_MIN]) / ([COMPLICATION.RANGED_VALUE_MAX] - [COMPLICATION.RANGED_VALUE_MIN])"
        def prog(themed: bool) -> str:
            t = f'<Transform target="color" value="extractColorFromColors({rv_ramp}, false, 0.05)"/>' if themed else ""
            return f'''<PartDraw x="0" y="0" width="{w}" height="{h}">
  <Arc centerX="{cx}" centerY="{cy}" width="{d}" height="{d}" startAngle="-140" endAngle="140">
    <Stroke thickness="7" color="#FF262320" cap="ROUND"/>
  </Arc>
  <Arc centerX="{cx}" centerY="{cy}" width="{d}" height="{d}" startAngle="-140" endAngle="140">
    <Stroke thickness="7" color="[CONFIGURATION.accent.0]" cap="ROUND">{t}</Stroke>
    <Transform target="endAngle" value="-140 + 280 * ({frac})"/>
  </Arc>
</PartDraw>'''
        body = f'''<Condition>
  <Expressions><Expression name="ramp"><![CDATA[{rv_ramp} != null]]></Expression></Expressions>
  <Compare expression="ramp">
{ind(prog(True), 4)}
  </Compare>
  <Default>
{ind(prog(False), 4)}
  </Default>
</Condition>
{plain_text(8, cy - 18, w - 16, 36, 26, "#FFF5F1EA", "[COMPLICATION.TEXT]")}'''
        comps.append(f'<Complication type="RANGED_VALUE">\n{ind(body, 2)}\n</Complication>')
    if "LONG_TEXT" in types:
        comps.append(f'<Complication type="LONG_TEXT">\n{ind(plain_text(0, (h - 34) // 2, w, 34, 24, "#FFF5F1EA", "[COMPLICATION.TEXT]"), 2)}\n</Complication>')
    if "MONOCHROMATIC_IMAGE" in types:
        sz = min(w, h) - 24
        comps.append(f'''<Complication type="MONOCHROMATIC_IMAGE">
  <PartImage x="{cx - sz // 2}" y="{cy - sz // 2}" width="{sz}" height="{sz}" tintColor="[CONFIGURATION.accent.0]">
    <Image resource="[COMPLICATION.MONOCHROMATIC_IMAGE]"/>
  </PartImage>
</Complication>''')
    if "SMALL_IMAGE" in types:
        sz = min(w, h) - 16
        comps.append(f'''<Complication type="SMALL_IMAGE">
  <PartImage x="{cx - sz // 2}" y="{cy - sz // 2}" width="{sz}" height="{sz}">
    <Image resource="[COMPLICATION.SMALL_IMAGE]"/>
  </PartImage>
</Complication>''')
    comps.append('<Complication type="EMPTY"/>')
    return f'''<ComplicationSlot x="{x}" y="{y}" width="{w}" height="{h}" slotId="{slot}" displayName="{s(name, label)}" supportedTypes="{" ".join(types)} EMPTY">
  <Variant mode="AMBIENT" target="alpha" value="0"/>
  <DefaultProviderPolicy defaultSystemProvider="{default[0]}" defaultSystemProviderType="{default[1]}"{pp}/>
  <BoundingBox x="0" y="0" width="{w}" height="{h}"/>
{ind(chr(10).join(comps), 2)}
</ComplicationSlot>'''


def plain_text(x: int, y: int, w: int, h: int, size: int, color: str, expr: str) -> str:
    return f'''<PartText x="{x}" y="{y}" width="{w}" height="{h}">
  <Text align="CENTER" ellipsis="TRUE">
    <Font family="SYNC_TO_DEVICE" size="{size}" weight="NORMAL" slant="NORMAL" color="{color}">
      <Template>%s<Parameter expression="{expr}"/></Template>
    </Font>
  </Text>
</PartText>'''


def config() -> str:
    def lst(id: str, label: str, opts: list[tuple], default: int = 0) -> str:
        o = "".join(f'\n  <ListOption id="{i}" displayName="{s(f"{id}_{k}", n)}" screenReaderText="{s(f"{id}_{k}", n)}"/>' for i, (k, n, *_) in enumerate(opts))
        return f'<ListConfiguration id="{id}" displayName="{s("cfg_" + id, label)}" screenReaderText="{s("cfg_" + id, label)}" defaultValue="{default}">{o}\n</ListConfiguration>'
    def boo(id: str, label: str, default: str) -> str:
        return f'<BooleanConfiguration id="{id}" displayName="{s("cfg_" + id, label)}" screenReaderText="{s("cfg_" + id, label)}" defaultValue="{default}"/>'
    acc = "".join(f'\n  <ColorOption id="{i}" displayName="{s("accent_" + k, n)}" screenReaderText="{s("accent_" + k, n)}" colors="{c}"/>' for i, (k, n, c) in enumerate(ACCENTS))
    return "\n".join([
        lst("ringData", "Ring", RING_DATA), lst("ringWidth", "Ring width", RING_WIDTH, 1), boo("ambientRing", "Ring when dimmed", "FALSE"),
        lst("timeStyle", "Time style", TIME_STYLES), lst("timeColor", "Time colour", TIME_COLORS),
        boo("showDate", "Date", "TRUE"), lst("dateStyle", "Date style", DATE_STYLES),
        boo("showStatus", "Claude status line", "TRUE"), boo("ticks", "Hour marks", "FALSE"),
        lst("background", "Background", BACKGROUNDS),
        f'<ColorConfiguration id="accent" displayName="{s("cfg_accent", "Colour without Hearth")}" screenReaderText="{s("cfg_accent", "Colour without Hearth")}" defaultValue="0">{acc}\n</ColorConfiguration>',
    ])


def main() -> None:
    subs = [
        sub_slot(1, 88, 304, 120, 84, "slot_left", "Left", ["SHORT_TEXT", "RANGED_VALUE", "MONOCHROMATIC_IMAGE", "SMALL_IMAGE"], ("STEP_COUNT", "SHORT_TEXT"), ("WeekComplication", "RANGED_VALUE")),
        sub_slot(2, 242, 304, 120, 84, "slot_right", "Right", ["SHORT_TEXT", "RANGED_VALUE", "MONOCHROMATIC_IMAGE", "SMALL_IMAGE"], ("WATCH_BATTERY", "RANGED_VALUE"), ("BusyComplication", "SHORT_TEXT")),
        sub_slot(3, 155, 24, 140, 44, "slot_top", "Top", ["SHORT_TEXT", "MONOCHROMATIC_IMAGE"], ("EMPTY", "EMPTY")),
        sub_slot(4, 145, 392, 160, 40, "slot_bottom", "Bottom", ["SHORT_TEXT", "LONG_TEXT", "MONOCHROMATIC_IMAGE"], ("EMPTY", "EMPTY")),
    ]
    ambient = '''<!-- the always-on screen: only a thin time (and the ring, when you switch that on) -->
<DigitalClock x="0" y="0" width="450" height="450" alpha="0">
  <Variant mode="AMBIENT" target="alpha" value="255"/>
  <TimeText format="hh:mm" hourFormat="SYNC_TO_DEVICE" align="CENTER" x="25" y="140" width="400" height="120">
    <Font family="SYNC_TO_DEVICE" size="100" weight="THIN" slant="NORMAL" color="#FFFFFFFF"/>
  </TimeText>
</DigitalClock>'''
    cfg = config()   # first, so the option names exist before the slots name theirs
    scene = "\n".join([hearth_slot(), ambient] + subs)
    xml = f'''<?xml version="1.0" encoding="utf-8"?>
<!-- Made by watchface/gen.py: change that file, not this one. -->
<WatchFace width="450" height="450">
  <Metadata key="CLOCK_TYPE" value="DIGITAL"/>
  <Metadata key="PREVIEW_TIME" value="10:08:32"/>
  <UserConfigurations>
{ind(cfg, 4)}
  </UserConfigurations>
  <Scene backgroundColor="#ff000000">
{ind(scene, 4)}
  </Scene>
</WatchFace>
'''
    os.makedirs(os.path.join(RES, "raw"), exist_ok=True)
    open(os.path.join(RES, "raw/watchface.xml"), "w").write(xml)
    strings["face_name"] = "Hearth"
    esc = lambda t: t.replace("&", "&amp;").replace("<", "&lt;").replace("'", "\\'")
    open(os.path.join(RES, "values/strings.xml"), "w").write("<resources>\n" + "".join(f'    <string name="{k}">{esc(v)}</string>\n' for k, v in sorted(strings.items())) + "</resources>\n")
    print(f"watchface.xml: {len(xml) // 1024} KB, {len(strings)} strings")


if __name__ == "__main__":
    main()
