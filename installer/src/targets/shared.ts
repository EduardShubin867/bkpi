import { access, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { binaryPath, packageRoot, run, type Runner } from '../runtime.js';
export interface Target {name:string;skillDirectory:string;add:string[];remove:string[];get:string[]}
export async function exists(path:string):Promise<boolean>{try{await access(path);return true;}catch(e){if((e as NodeJS.ErrnoException).code==='ENOENT')return false;throw e;}}
export async function install(target:Target,runner:Runner=run):Promise<void>{
  runner(target.name,['--version'],true);
  const marker=join(target.skillDirectory,'.bkpi-managed');
  const owned=await exists(marker);
  if(await exists(target.skillDirectory) && !owned)throw new Error(`Existing skill directory is not managed by BKPI: ${target.skillDirectory}`);
  let registered=false;
  try{runner(target.name,target.get,true);registered=true;}catch{/* Absent entry; the CLI availability was checked above. */}
  if(registered && !owned)throw new Error('Existing bkpi MCP registration is not managed by this installer; resolve it before installing');
  const skill=await readFile(join(packageRoot,'bundles','skill','SKILL.md'),'utf8');
  await mkdir(target.skillDirectory,{recursive:true});
  await writeFile(join(target.skillDirectory,'SKILL.md'),skill);
  await writeFile(marker,'BKPI installer owns this directory and bkpi MCP registration.\n');
  if(!registered){
    try{runner(target.name,[...target.add,'--',binaryPath,'mcp'],true);}
    catch(e){if(!owned)await rm(target.skillDirectory,{recursive:true,force:true});throw e;}
  }
}
export async function uninstall(target:Target,runner:Runner=run):Promise<void>{
  if(!await exists(join(target.skillDirectory,'.bkpi-managed')))throw new Error('Refusing to remove an unmanaged agent target');
  runner(target.name,target.remove,true);
  await rm(target.skillDirectory,{recursive:true});
}
