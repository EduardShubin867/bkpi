import { createHash } from 'node:crypto';
import { chmod, copyFile, mkdir, readFile, rename, rm, writeFile } from 'node:fs/promises';
import { homedir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import spawn from 'cross-spawn';
export const packageRoot = fileURLToPath(new URL('../', import.meta.url));
export const packageInfo = JSON.parse(await readFile(join(packageRoot, 'package.json'), 'utf8')) as {name:string;version:string;bkpi:{releaseRepository:string}};
export const installRoot = process.env.BKPI_INSTALL_DIR || (process.platform==='win32' ? join(process.env.LOCALAPPDATA || join(homedir(),'AppData','Local'),'bkpi') : join(homedir(), '.local', 'share', 'bkpi'));
export const binaryPath = join(installRoot, 'bin', process.platform === 'win32' ? 'bkpi.exe' : 'bkpi');
export function redact(text:string):string {
  return text.replace(/https?:\/\/[^\s"<>]+\/rest\/\d+\/[^/\s"<>]+\/?/gi,'[REDACTED WEBHOOK]').replace(/sk-or-[A-Za-z0-9_-]+/g,'[REDACTED KEY]');
}
// Onboarding secrets travel only through stdin. Capture both streams so failed
// child processes cannot leak echoed input or raw HTTP errors to the terminal.
export function runCredential(args:string[],input=''):string {
  const result=spawn.sync(binaryPath,args,{stdio:['pipe','pipe','pipe'],input,encoding:'utf8',shell:false});
  if(result.error || result.status!==0) throw new Error('Credential operation failed. Details suppressed; run bkpi doctor for safe diagnostics.');
  let output=(result.stdout || '')+(result.stderr || '');
  for(const value of input.split('\n').filter(Boolean)){
    output=output.split(value).join('[REDACTED]');
    try{for(const part of new URL(value).pathname.split('/').slice(3).filter(Boolean))output=output.split(part).join('[REDACTED]');}catch{/* Input may be an API key, not a URL. */}
  }
  return redact(output);
}
export type Runner = (command:string,args:string[],capture?:boolean)=>string;
export const run:Runner = (command,args,capture=false) => {
  const result=spawn.sync(command,args,{stdio:capture?['ignore','pipe','pipe']:'inherit',encoding:'utf8',shell:false});
  if(result.error || result.status!==0) throw new Error(`Command failed: ${command}. Check that it is installed and available. Details suppressed.`);
  return redact(result.stdout || '');
};
export function target(platform:string,arch:string):string {
  const targets:Record<string,string>={ 'darwin-arm64':'aarch64-apple-darwin','darwin-x64':'x86_64-apple-darwin','linux-x64':'x86_64-unknown-linux-gnu','linux-arm64':'aarch64-unknown-linux-gnu','win32-x64':'x86_64-pc-windows-msvc' };
  const value=targets[`${platform}-${arch}`];if(!value)throw new Error(`Unsupported platform: ${platform}/${arch}`);return value;
}
export function assetName(version:string,platform=process.platform,arch=process.arch):string {
  if(!/^\d+\.\d+\.\d+(?:-[a-zA-Z0-9.-]+)?$/.test(version))throw new Error('Invalid release version');
  return `bkpi-v${version}-${target(platform,arch)}${platform==='win32'?'.exe':''}`;
}
export function verify(bytes:Uint8Array,checksum:string):void {
  if(!/^[a-f0-9]{64}$/i.test(checksum) || createHash('sha256').update(bytes).digest('hex')!==checksum.toLowerCase())throw new Error('SHA256 mismatch; runtime was not installed');
}
async function download(url:string):Promise<Uint8Array> {
  const response=await fetch(url,{signal:AbortSignal.timeout(120000)});
  if(!response.ok)throw new Error(`Release asset unavailable (HTTP ${response.status})`);
  return new Uint8Array(await response.arrayBuffer());
}
export async function installRuntime():Promise<void> {
  await mkdir(dirname(binaryPath),{recursive:true});
  const temp=`${binaryPath}.${process.pid}.tmp${process.platform === "win32" ? ".exe" : ""}`;
  try {
    if(process.env.BKPI_RUNTIME_SOURCE) {
      // Explicit local development override, never an automatic download fallback.
      await copyFile(process.env.BKPI_RUNTIME_SOURCE,temp);
    } else {
      const repo=process.env.BKPI_RELEASE_REPOSITORY || packageInfo.bkpi.releaseRepository;
      if(!/^[\w.-]+\/[\w.-]+$/.test(repo))throw new Error('Set BKPI_RELEASE_REPOSITORY=owner/repository; this source tree has no configured GitHub remote yet');
      const asset=assetName(packageInfo.version);
      const base=`https://github.com/${repo}/releases/download/v${packageInfo.version}`;
      const [bytes,sum]=await Promise.all([download(`${base}/${asset}`),download(`${base}/${asset}.sha256`)]);
      verify(bytes,new TextDecoder().decode(sum).trim().split(/\s+/)[0] || '');
      await writeFile(temp,bytes,{mode:0o755});
    }
    await chmod(temp,0o755);
    const version=run(temp,['--version'],true).trim();
    if(version!==`bkpi ${packageInfo.version}`)throw new Error('Downloaded runtime version does not match installer');
    await rename(temp,binaryPath);
  } finally {await rm(temp,{force:true});}
}
export interface InstallState { version:1; targets:string[] }
export async function loadState():Promise<InstallState> {
  try {const value=JSON.parse(await readFile(join(installRoot,'install.json'),'utf8')) as InstallState;
    if(value.version!==1 || !Array.isArray(value.targets) || value.targets.some(t=>!['codex','claude'].includes(t)))throw new Error('Invalid installer state');return value;
  }catch(e){if((e as NodeJS.ErrnoException).code==='ENOENT')return {version:1,targets:[]};throw e;}
}
export async function saveState(state:InstallState):Promise<void> {
  await mkdir(installRoot,{recursive:true});const temp=join(installRoot,`install.${process.pid}.tmp`);
  await writeFile(temp,JSON.stringify(state,null,2),{mode:0o600});await rename(temp,join(installRoot,'install.json'));
}
