import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtemp, readFile, rm, writeFile, mkdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { assetName, target, verify, type Runner } from '../src/runtime.js';
import { install, uninstall, type Target } from '../src/targets/shared.js';
import { codex } from '../src/targets/codex.js';
import { claude } from '../src/targets/claude.js';
test('release target naming and checksum fail closed',()=>{
 assert.equal(target('darwin','arm64'),'aarch64-apple-darwin');
 assert.equal(target('linux','arm64'),'aarch64-unknown-linux-gnu');
 assert.equal(assetName('0.6.0','win32','x64'),'bkpi-v0.6.0-x86_64-pc-windows-msvc.exe');
 assert.throws(()=>target('freebsd','x64'));assert.throws(()=>assetName('../evil'));
 const bytes=Buffer.from('fixture');verify(bytes,createHash('sha256').update(bytes).digest('hex'));assert.throws(()=>verify(bytes,'a'.repeat(64)));
});
test('target adapters use official CLI arrays, no shell or secret parameters',()=>{
 assert.deepEqual(codex.add,['mcp','add','bkpi']);assert.deepEqual(claude.add,['mcp','add','--transport','stdio','--scope','user','bkpi']);assert.ok(codex.skillDirectory.endsWith(join('.agents','skills','bkpi')));assert.ok(claude.skillDirectory.endsWith(join('.claude','skills','bkpi')));
});
test('managed install is repeatable, uninstall removes only owned target',async()=>{
 const dir=await mkdtemp(join(tmpdir(),'bkpi-installer-'));
 try{
  const t:Target={...codex,skillDirectory:join(dir,'skill')};const calls:string[][]=[];
  const runner:Runner=(cmd,args)=>{calls.push([cmd,...args]);if(args.includes('get'))throw new Error('absent');return '';};
  await install(t,runner);await install(t,runner);
  assert.match(await readFile(join(t.skillDirectory,'SKILL.md'),'utf8'),/No MCP tool changes tasks/);
  assert.equal(calls.filter(c=>c.includes('add')).length,2);
  await uninstall(t,runner);await assert.rejects(uninstall(t,runner));
 }finally{await rm(dir,{recursive:true,force:true});}
});
test('unmanaged skill and MCP registrations are preserved',async()=>{
 const dir=await mkdtemp(join(tmpdir(),'bkpi-unmanaged-'));
 try{
  const t:Target={...claude,skillDirectory:join(dir,'skill')};await mkdir(t.skillDirectory);await writeFile(join(t.skillDirectory,'SKILL.md'),'original');
  await assert.rejects(install(t,()=>''),/not managed/);assert.equal(await readFile(join(t.skillDirectory,'SKILL.md'),'utf8'),'original');
  await rm(t.skillDirectory,{recursive:true});await assert.rejects(install(t,()=>''),/not managed/);
 }finally{await rm(dir,{recursive:true,force:true});}
});

test('repeat install does not re-register an existing managed server',async()=>{
 const dir=await mkdtemp(join(tmpdir(),'bkpi-repeat-'));let registered=false;let additions=0;
 try{
  const t:Target={...codex,skillDirectory:join(dir,'skill')};
  const runner:Runner=(_cmd,args)=>{if(args.includes('get')&&!registered)throw new Error('missing');if(args.includes('add')){registered=true;additions++;}return '';};
  await install(t,runner);await install(t,runner);assert.equal(additions,1);
 }finally{await rm(dir,{recursive:true,force:true});}
});
test('failed registration leaves no managed directory on a new target',async()=>{
 const dir=await mkdtemp(join(tmpdir(),'bkpi-fail-'));
 try{
  const t:Target={...claude,skillDirectory:join(dir,'skill')};
  const runner:Runner=(_cmd,args)=>{if(args.includes('get')||args.includes('add'))throw new Error('failure');return '';};
  await assert.rejects(install(t,runner));await assert.rejects(readFile(join(t.skillDirectory,'.bkpi-managed')));
 }finally{await rm(dir,{recursive:true,force:true});}
});

test('all skill bundles retain chat KPI fallback and explicit local write consent',async()=>{
 const source=await readFile(new URL('../../agent/skill/SKILL.md',import.meta.url),'utf8');
 for(const relative of ['skill/SKILL.md','codex/bkpi/skills/bkpi/SKILL.md','claude/bkpi/skills/bkpi/SKILL.md']){
  const bundled=await readFile(new URL(`../bundles/${relative}`,import.meta.url),'utf8');
  assert.equal(bundled,source);
  assert.match(bundled,/workspace is not broken/);
  assert.match(bundled,/Write only after explicit user consent/);
 }
});
