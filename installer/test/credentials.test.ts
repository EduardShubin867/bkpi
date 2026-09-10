import {test} from 'node:test';
import assert from 'node:assert/strict';
import * as p from '@clack/prompts';
import {addIntegration,moveCredential,backendOptions,type Backend,type Integration} from '../src/credentials.js';
const secret='https://portal.test/rest/1/fixture-token/';
function fixture(backend:Backend,existing?:Integration){
 const calls:{args:string[];input:string}[]=[];const logs:string[]=[];let selects=0;let passwords=0;
 const prompts={...p,
  select:async(options:{initialValue:string})=>{selects++;assert.equal(options.initialValue,existing?.credential.store==='keyring'?'keychain':existing?.credential.store || 'file');return backend;},
  password:async()=>{passwords++;return secret;},
  text:async(options:{message:string})=>options.message.includes('origin')?'https://portal.test':options.message.includes('variable')?'BKPI_TEST_WEBHOOK':'Астро-Волга',
  confirm:async()=>true,
  log:{...p.log,info:(text:string)=>logs.push(text),success:(text:string)=>logs.push(text)},
 } as unknown as typeof p;
 return {prompts,calls,logs,selects:()=>selects,passwords:()=>passwords,execute:(args:string[],input='')=>{calls.push({args,input});return '✓ Credential saved securely';}};
}
for(const backend of ['file','keychain','env'] as const){
 test(`setup selects ${backend}; file is default and secret stays out of argv`,async()=>{
  const f=fixture(backend);await addIntegration(undefined,f.prompts,f.execute);
  assert.equal(backendOptions[0].value,'file');assert.equal(f.selects(),1);
  assert.equal(f.calls[0].args[3],backend);assert.ok(!f.calls[0].args.join(' ').includes('fixture-token'));
  assert.equal(f.passwords(),backend==='env'?0:1);assert.equal(f.calls[0].input,backend==='env'?'':secret+'\n');
  assert.ok(!f.logs.join(' ').includes('fixture-token'));
  if(backend==='env')assert.ok(f.logs.join(' ').includes('not usable'));
 });
}
for(const store of ['file','keyring','env'] as const){
 test(`repeated setup preserves ${store} without asking for storage again`,async()=>{
  const existing:Integration={id:'portal',display_name:'Астро-Волга',base_url:'https://portal.test',credential:{store,account:'BKPI_TEST_WEBHOOK'}};
  const backend=store==='keyring'?'keychain':store;const f=fixture(backend,existing);
  await addIntegration(existing,f.prompts,f.execute);assert.equal(f.selects(),0);assert.equal(f.calls[0].args[3],backend);assert.ok(f.calls[0].args.includes('portal'));
 });
}
test('env move requires explicit confirmation and cancellation leaves config alone',async()=>{
 const existing:Integration={id:'portal',display_name:'Portal',base_url:'https://portal.test',credential:{store:'file',account:'bitrix-portal'}};
 const f=fixture('env',existing);await moveCredential(existing,f.prompts,f.execute);assert.ok(f.calls[0].args.includes('--confirm-env'));assert.equal(f.passwords(),0);
 const cancelled={...f.prompts,confirm:async()=>false} as unknown as typeof p;
 await moveCredential(existing,cancelled,f.execute);assert.equal(f.calls.length,1);
});
