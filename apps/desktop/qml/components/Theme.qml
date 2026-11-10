pragma Singleton
import QtQuick
// Frosted Obsidian Glass tokens from docs/mockup/source/System.dc.html.
QtObject {
    // Surfaces (L0 base, L1 pane, L2 glass, L3 overlay).
    readonly property color background: "#0b0b0f"      // surface.base
    readonly property color raisedBase: "#121218"      // surface.raised
    readonly property color graphite: "#1a1a22"        // surface.graphite
    readonly property color elevated: "#292932"        // surface.elevated
    readonly property color pane: "#b8121218"          // L1 rgba(18,18,24,.72)
    readonly property color sidebar: "#9e121218"       // rgba(18,18,24,.62)
    readonly property color header: "#c70b0b0f"        // rgba(11,11,15,.78)
    readonly property color glassTop: "#cc292932"      // rgba(41,41,50,.80)
    readonly property color glassBottom: "#db1a1a22"   // rgba(26,26,34,.86)
    readonly property color overlay: "#f21e1e26"       // L3 rgba(30,30,38,.95)
    readonly property color hairline: "#0fffffff"      // white 6%
    readonly property color hairline5: "#0dffffff"     // white 5%
    readonly property color line7: "#12ffffff"         // white 7%
    readonly property color line8: "#14ffffff"         // white 8%
    readonly property color line9: "#17ffffff"         // white 9%
    readonly property color line10: "#1affffff"        // white 10%
    readonly property color line16: "#29ffffff"        // white 16%
    readonly property color fill3: "#09ffffff"         // white 3.5%
    readonly property color fill4: "#0bffffff"         // white 4.5%
    readonly property color fill5: "#0dffffff"         // white 5%
    readonly property color fill6: "#0fffffff"         // white 6%
    readonly property color fill7: "#12ffffff"         // white 7%
    readonly property color fill9: "#17ffffff"         // white 9%
    readonly property color fill10: "#1affffff"        // white 10%
    // Accent.
    readonly property color accent: "#ff681f"          // accent.primary
    readonly property color accentStrong: "#ff5e14"    // accent.strong
    readonly property color accentTop: "#ff7c36"
    readonly property color accentHoverTop: "#ff8c4a"
    readonly property color accentHoverBottom: "#ff6a20"
    readonly property color ember: "#ffac52"           // accent.ember: focus and highlights
    readonly property color accentLink: "#ff8a3d"
    readonly property color accentText: "#170904"      // text.on-accent
    readonly property color accentSoft: "#24ff681f"    // rgba(255,104,31,.14)
    readonly property color accentBorder: "#8cff7832"  // rgba(255,120,50,.55)
    readonly property color accentChipText: "#ffd9bd"
    // Text.
    readonly property color ink: "#f5f5f7"             // text.primary
    readonly property color secondary: "#a3a3af"       // text.secondary
    readonly property color muted: "#8b8b97"           // text.muted
    readonly property color soft: "#c9c9d2"
    // Status.
    readonly property color success: "#3dd68c"
    readonly property color successText: "#5be3a0"
    readonly property color warning: "#f5c451"
    readonly property color danger: "#ff8a8d"
    readonly property color dangerFill: "#c7363b"
    readonly property color closeHover: "#e5484d"
    readonly property color info: "#6cb4ff"
    // Compatibility names used by earlier components and the contrast check.
    readonly property color surface: "#191a23"         // composited L1 pane on base
    readonly property color raised: "#22232e"
    readonly property color outline: "#353642"
    readonly property color control: "#70717f"         // input boundaries: at least 3:1 everywhere
    readonly property color selected: "#30231e"
    readonly property color errorSurface: "#302025"
    readonly property color errorInk: "#ff8a8d"
    // Radii and space (4 px base).
    readonly property int radiusXs: 5
    readonly property int radiusSm: 8
    readonly property int radiusMd: 10
    readonly property int radiusLg: 12
    readonly property int radiusXl: 16
    readonly property int radius2xl: 20
    // Motion.
    readonly property int instant: 120
    readonly property int fast: 170
    readonly property int base: 240
    readonly property int slow: 300
    // Typography: Inter Tight for display, Inter for interface, JetBrains Mono for
    // anything a user might copy. Installed families are used when present.
    readonly property string family: "Inter"
    readonly property var installedFamilies: Qt.fontFamilies()
    readonly property string display: installedFamilies.indexOf("Inter Tight") >= 0 ? "Inter Tight" : family
    readonly property real displayTracking: display === "Inter Tight" ? 0 : 0.008
    readonly property string mono: installedFamilies.indexOf("JetBrains Mono") >= 0 ? "JetBrains Mono"
        : (installedFamilies.indexOf("JetBrainsMono Nerd Font") >= 0 ? "JetBrainsMono Nerd Font" : "monospace")
}
