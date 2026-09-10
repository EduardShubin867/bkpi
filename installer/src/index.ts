#!/usr/bin/env node
import * as p from '@clack/prompts';
import { rm } from 'node:fs/promises';
import { dirname } from 'node:path';
import { redact, binaryPath, installRuntime, loadState, packageInfo, run, saveState } from './runtime.js';
import { codex } from './targets/codex.js';
import { claude } from './targets/claude.js';
import { exists, install, uninstall } from './targets/shared.js';
import { addIntegration, moveCredential, type Integration } from './credentials.js';
import { pathInstruction } from './targets/cli.js';
function answer<T>(value:T|symbol):T{if(p.isCancel(value))throw new Error('Cancelled');return value as T;}
const targets={codex,claude};
type TargetName=keyof typeof targets;
async function chooseTargets():Promise<void>{
  const state=await loadState();
  const selected=answer(await p.multiselect({message:'Agent targets (empty = CLI only)',options:[{value:'codex' as const,label:'Codex'},{value:'claude' as const,label:'Claude Code'}],initialValues:state.targets as TargetName[],required:false}));
  for(const name of state.targets.slice() as TargetName[]){if(!selected.includes(name)){await uninstall(targets[name]);state.targets=state.targets.filter(t=>t!==name);await saveState(state);}}
  for(const name of selected){await install(targets[name]);if(!state.targets.includes(name))state.targets.push(name);await saveState(state);}
}
async function main():Promise<void>{
  const command=process.argv[2];
  if(command==='--version'){console.log(`${packageInfo.name} installer ${packageInfo.version}`);return;}
  if(command==='--help' || command==='help'){console.log('BKPI installer: setup | doctor | update | uninstall | init | targets | integration <runtime args> | person kpi set/show\nLocal dev: BKPI_RUNTIME_SOURCE=/absolute/path/to/bkpi\nRelease repository: BKPI_RELEASE_REPOSITORY=owner/repo');return;}
  if(command==='doctor'){
    console.log(run(binaryPath,['--version'],true).trim());
    const state=await loadState();
    for(const name of state.targets as TargetName[]){run(name,targets[name].get,true);console.log(`✓ ${name} MCP registered`);}
    console.log(pathInstruction());run(binaryPath,['doctor']);return;
  }
  if(command==='uninstall'){
    const state=await loadState();
    for(const name of state.targets.slice() as TargetName[]){await uninstall(targets[name]);state.targets=state.targets.filter(t=>t!==name);await saveState(state);}
    await rm(dirname(binaryPath),{recursive:true,force:true});
    console.log('Runtime and managed agent targets removed. Credentials, global configuration and workspaces are retained.');return;
  }
  if(command==='update'){await installRuntime();console.log(`✓ Runtime ${packageInfo.version} installed. ${pathInstruction()}`);return;}
  if(!await exists(binaryPath))await installRuntime();
  if(command==='init'){run(binaryPath,['init',...process.argv.slice(3)]);return;}
  if(command && ['integration','config','person'].includes(command)){run(binaryPath,[command,...process.argv.slice(3)]);return;}
  if(command==='targets'){await chooseTargets();return;}
  if(command && command!=='setup')throw new Error('Unknown command; run with --help');
  p.intro('BKPI setup');
  const state=await loadState();
  p.log.info(`Runtime: ${binaryPath}; targets: ${state.targets.join(', ') || 'CLI only'}`);
  const action=answer(await p.select({message:'What would you like to do?',options:[{value:'add',label:'Add Bitrix integration'},{value:'webhook',label:'Update an existing webhook'},{value:'storage',label:'Change credential storage'},{value:'targets',label:'Add or remove agent targets'},{value:'remove',label:'Remove integration'},{value:'update',label:'Update runtime'},{value:'migrate',label:'Migrate legacy config'},{value:'done',label:'Finish'}]}));
  if(action==='add'){
    do{await addIntegration();}while(answer(await p.confirm({message:'Add another Bitrix integration?',initialValue:false})));
    await chooseTargets();
  }else if(action==='webhook' || action==='storage'){
    const integrations=JSON.parse(run(binaryPath,['integration','list'],true)) as Integration[];
    if(!integrations.length)throw new Error('No integrations configured');
    const id=answer(await p.select({message:'Bitrix integration',options:integrations.map(i=>({value:i.id,label:i.display_name,hint:`credential: ${i.credential.store}`}))}));
    const existing=integrations.find(i=>i.id===id)!;
    console.log(run(binaryPath,['integration','credential','show',id,'--json'],true));
    if(action==='storage')await moveCredential(existing);else await addIntegration(existing);
  }else if(action==='targets'){await chooseTargets();}
  else if(action==='remove'){
    const integrations=JSON.parse(run(binaryPath,['integration','list'],true)) as Integration[];
    if(!integrations.length)throw new Error('No integrations configured');
    const id=answer(await p.select({message:'Remove integration',options:integrations.map(i=>({value:i.id,label:i.display_name,hint:`credential: ${i.credential.store}`}))}));run(binaryPath,['integration','remove',id]);
  }else if(action==='update'){await installRuntime();}
  else if(action==='migrate'){run(binaryPath,['config','migrate']);}
  p.outro(`Ready. ${pathInstruction()} Run ${packageInfo.name} init in your project.`);
}
main().catch((error:unknown)=>{console.error(error instanceof Error ? redact(error.message) : 'Setup failed');console.error('BKPI setup did not complete. Check the runtime/agent CLI availability and setup inputs. No credentials are printed. For release downloads, configure BKPI_RELEASE_REPOSITORY; for local testing use BKPI_RUNTIME_SOURCE.');process.exitCode=1;});
