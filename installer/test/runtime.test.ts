import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp,rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join,resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
// Run module tests in a subprocess so installation roots never touch the real home.
test('release download verifies checksum before executing or replacing a runtime',async()=>{
 const dir=await mkdtemp(join(tmpdir(),'bkpi-checksum-'));
 try{
  const script=`
   import {writeFile,readFile,mkdir} from 'node:fs/promises';
   const runtime=await import('./src/runtime.ts');
   await mkdir(${JSON.stringify(join(dir,'bin'))},{recursive:true});
   await writeFile(runtime.binaryPath,'existing runtime');
   globalThis.fetch=async (url)=>new Response(String(url).endsWith('.sha256')?'0'.repeat(64):'corrupt binary');
   let failed=false;try{await runtime.installRuntime();}catch(e){failed=String(e).includes('SHA256 mismatch');}
   if(!failed || await readFile(runtime.binaryPath,'utf8')!=='existing runtime')process.exit(2);
  `;
  const result=spawnSync(process.execPath,['--import','tsx','--input-type=module','-e',script],{cwd:resolve('.'),env:{...process.env,BKPI_INSTALL_DIR:dir,BKPI_RELEASE_REPOSITORY:'owner/repo',BKPI_RUNTIME_SOURCE:''},encoding:'utf8'});
  assert.equal(result.status,0,result.stderr);
 }finally{await rm(dir,{recursive:true,force:true});}
});

test('installer forwards person KPI commands and space-containing paths without a shell',{skip:process.platform==='win32'},async()=>{
 const dir=await mkdtemp(join(tmpdir(),'bkpi-routing-'));
 try{
  const {mkdir,writeFile}=await import('node:fs/promises');
  await mkdir(join(dir,'bin'));
  await writeFile(join(dir,'bin','bkpi'),'#!/usr/bin/env node\nconsole.log(JSON.stringify(process.argv.slice(2)));\n',{mode:0o755});
  const args=['person','kpi','set','employee','/tmp/my KPI.txt','--json'];
  const result=spawnSync(process.execPath,['--import','tsx','src/index.ts',...args],{cwd:resolve('.'),env:{...process.env,BKPI_INSTALL_DIR:dir},encoding:'utf8'});
  assert.equal(result.status,0,result.stderr);
  assert.deepEqual(JSON.parse(result.stdout),args);
 }finally{await rm(dir,{recursive:true,force:true});}
});

test('credential child failure suppresses both output streams and success redacts echoed stdin',{skip:process.platform==='win32'},async()=>{
 const dir=await mkdtemp(join(tmpdir(),'bkpi-secret-failure-'));
 try{
  const {mkdir,writeFile}=await import('node:fs/promises');await mkdir(join(dir,'bin'));
  for(const status of [1,0]){
   await writeFile(join(dir,'bin','bkpi'),`#!/usr/bin/env node\nlet input='';process.stdin.on('data',chunk=>input+=chunk);process.stdin.on('end',()=>{console.log(input);console.error('https://portal.test/rest/1/fixture-token/ sk-or-fixture');process.exit(${status});});\n`,{mode:0o755});
   const script=`const {runCredential}=await import('./src/runtime.ts');try{console.log(runCredential(['integration','add'],'arbitrary-fixture-secret\\n'));}catch(e){console.error(e.message);process.exitCode=1;}`;
   const result=spawnSync(process.execPath,['--import','tsx','--input-type=module','-e',script],{cwd:resolve('.'),env:{...process.env,BKPI_INSTALL_DIR:dir},encoding:'utf8'});
   assert.equal(result.status,status);for(const secret of ['arbitrary-fixture-secret','fixture-token','sk-or-fixture'])assert.ok(!(result.stdout+result.stderr).includes(secret),result.stdout+result.stderr);
  }
 }finally{await rm(dir,{recursive:true,force:true});}
});
