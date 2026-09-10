import { homedir } from 'node:os';
import { join } from 'node:path';
import type { Target } from './shared.js';
// Verified against official skill docs and installed `codex mcp add --help`.
export const codex:Target={name:'codex',skillDirectory:join(homedir(),'.agents','skills','bkpi'),add:['mcp','add','bkpi'],remove:['mcp','remove','bkpi'],get:['mcp','get','bkpi','--json']};
