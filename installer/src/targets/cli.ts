import { dirname } from 'node:path';
import { binaryPath } from '../runtime.js';
export function pathInstruction():string{return `Add ${dirname(binaryPath)} to your user PATH. Restart your terminal afterward. The agent registrations use the absolute runtime path.`;}
