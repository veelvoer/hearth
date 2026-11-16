//! The MCP server starter kit, written into a new project by "build a tool" (the files live in desktop/relay/mcpkit).
pub const FILES: [(&str, &str); 4] = [
    ("check.js", include_str!("../../relay/mcpkit/check.js")),
    ("register.js", include_str!("../../relay/mcpkit/register.js")),
    ("server.js", include_str!("../../relay/mcpkit/server.js")),
    ("tools.js", include_str!("../../relay/mcpkit/tools.js")),
];
