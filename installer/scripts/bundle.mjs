import { cp, mkdir, readFile, writeFile } from 'node:fs/promises';
const pkg=JSON.parse(await readFile(new URL('../package.json',import.meta.url),'utf8'));
const root=new URL('../bundles/',import.meta.url);
await mkdir(root,{recursive:true});
await cp(new URL('../../agent/skill',import.meta.url),new URL('skill',root),{recursive:true});
for(const target of ['codex','claude']){
  const plugin=new URL(`${target}/bkpi/`,root);
  await mkdir(new URL(`.${target}-plugin/`,plugin),{recursive:true});
  await cp(new URL('../../agent/skill',import.meta.url),new URL('skills/bkpi',plugin),{recursive:true});
  const manifest=target==='codex'?{name:'bkpi',version:pkg.version,description:pkg.description,author:{name:'BKPI contributors'},interface:{displayName:'BKPI',shortDescription:'KPI documents and Bitrix evidence for coding agents',longDescription:'Analyze each workspace person against their own KPI document using read-only Bitrix data and local evidence.',developerName:'BKPI contributors',category:'Productivity',capabilities:['Read','Write'],defaultPrompt:['Analyze my KPI evidence and gaps.']},skills:'./skills/',mcpServers:'./.mcp.json'}:{name:'bkpi',version:pkg.version,description:pkg.description};
  await writeFile(new URL(`.${target}-plugin/plugin.json`,plugin),JSON.stringify(manifest,null,2));
  await writeFile(new URL('.mcp.json',plugin),JSON.stringify({mcpServers:{bkpi:{command:'bkpi',args:['mcp']}}},null,2));
}
