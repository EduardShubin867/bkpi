import { homedir } from 'node:os';
import { join } from 'node:path';
import type { Target } from './shared.js';
// https://code.claude.com/docs/en/mcp and /skills
export const claude:Target={name:'claude',skillDirectory:join(homedir(),'.claude','skills','bkpi'),add:['mcp','add','--transport','stdio','--scope','user','bkpi'],remove:['mcp','remove','--scope','user','bkpi'],get:['mcp','get','bkpi']};
