import * as p from '@clack/prompts';
import { runCredential } from './runtime.js';
export type Backend='file'|'keychain'|'env';
export interface Integration {id:string;display_name:string;credential:{store:'file'|'keyring'|'keychain'|'env';account:string};base_url:string}
export const backendOptions=[
  {value:'file' as const,label:'Private local file',hint:'Simple and portable. Stored with owner-only permissions.'},
  {value:'keychain' as const,label:'OS secure credential store',hint:'Keychain / Credential Manager / Secret Service.'},
  {value:'env' as const,label:'Environment variables',hint:'Advanced / CI usage.'},
];
export function currentBackend(existing?:Integration):Backend {return existing?.credential.store==='keyring'?'keychain':existing?.credential.store || 'file';}
function answer<T>(value:T|symbol):T {if(p.isCancel(value))throw new Error('Cancelled');return value as T;}
export async function addIntegration(existing?:Integration, prompts=p, execute=runCredential):Promise<void> {
  const backend=existing?currentBackend(existing):answer(await prompts.select({message:'Where should BKPI store credentials?',options:backendOptions,initialValue:'file'}));
  const args=['integration','add','--backend',backend,...(existing?['--id',existing.id]:[])];
  let input='';
  if(backend==='env'){
    const origin=answer(await prompts.text({message:'Bitrix HTTPS origin (no webhook path)',initialValue:existing?.base_url,validate:value=>{try{const url=new URL(value || '');if(url.protocol==='https:' && url.origin===value)return;}catch{/* invalid input */}return 'Use only the HTTPS origin';}}));
    const variable=answer(await prompts.text({message:'Environment variable name',initialValue:existing?.credential.account || `BKPI_BITRIX_${new URL(origin).hostname.replace(/[^A-Za-z0-9]/g,'_').toUpperCase()}_WEBHOOK`,validate:value=>/^[A-Za-z_][A-Za-z0-9_]*$/.test(value || '')?undefined:'Use a valid environment variable name'}));
    args.push('--base-url',origin,'--env-var',variable);
    prompts.log.info(`Set ${variable} in the BKPI/agent process environment. Integration is not usable until it is set. BKPI does not save the value or modify shell profiles.`);
  }else{
    const secret=answer(await prompts.password({message:'Bitrix incoming webhook',validate:value=>value?.trim()?undefined:'Webhook is required'}));
    input=secret+'\n';
  }
  const name=existing?.display_name ?? answer(await prompts.text({message:'Integration display name (optional)'}));
  args.push('--name',name || '');
  prompts.log.success(execute(args,input).trim());
}
export async function moveCredential(existing:Integration,prompts=p,execute=runCredential):Promise<void>{
  const backend=answer(await prompts.select({message:'Where should BKPI store credentials?',options:backendOptions,initialValue:currentBackend(existing)}));
  const args=['integration','credential','move',existing.id,'--backend',backend];
  if(backend==='env'){
    const name=answer(await prompts.text({message:'Environment variable name',initialValue:'BKPI_BITRIX_WEBHOOK',validate:value=>/^[A-Za-z_][A-Za-z0-9_]*$/.test(value || '')?undefined:'Use a valid environment variable name'}));
    prompts.log.info(`Set ${name} in the environment before continuing. BKPI verifies the variable and keeps the previous credential for recovery.`);
    if(!answer(await prompts.confirm({message:'Is the variable already set for this BKPI process?',initialValue:false})))return;
    args.push('--env-var',name,'--confirm-env');
  }
  prompts.log.success(execute(args).trim());
}
