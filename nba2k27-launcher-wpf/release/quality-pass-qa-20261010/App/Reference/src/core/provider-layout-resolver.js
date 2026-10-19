const path = require('node:path');
const fsp = require('node:fs/promises');
const { execFile } = require('node:child_process');
const { promisify } = require('node:util');

const execFileAsync = promisify(execFile);
const SCHEMA = 'nba2k27-provider-layout-v1';
const RVA = /^0x[0-9A-F]{8}$/;

function parseResolverOutput(output) {
  if (typeof output !== 'string') throw new Error('Provider resolver returned no text output.');
  const start = output.indexOf('{');
  const end = output.lastIndexOf('}');
  if (start < 0 || end < start) throw new Error('Provider resolver returned no JSON report.');
  let report;
  try {
    report = JSON.parse(output.slice(start, end + 1));
  } catch (error) {
    throw new Error(`Provider resolver returned invalid JSON: ${error.message}`);
  }
  return report;
}

function validateResolverReport(report) {
  if (!report || typeof report !== 'object' || Array.isArray(report)) {
    throw new Error('Provider resolver report is not an object.');
  }
  if (report.schema !== SCHEMA) throw new Error('Provider resolver schema is not supported.');
  if (typeof report.ok !== 'boolean') throw new Error('Provider resolver omitted its result state.');
  if (!RVA.test(report.timestamp || '') || !RVA.test(report.imageSize || '')) {
    throw new Error('Provider resolver returned an invalid build identity.');
  }
  if (!report.checks || typeof report.checks !== 'object') {
    throw new Error('Provider resolver omitted structural checks.');
  }
  if (!report.layout || typeof report.layout !== 'object') {
    throw new Error('Provider resolver omitted the resolved layout.');
  }
  const layoutValues = Object.values(report.layout);
  if (layoutValues.length !== 20 || layoutValues.some((value) => !RVA.test(value))) {
    throw new Error('Provider resolver returned an incomplete layout.');
  }
  if (!Array.isArray(report.vtable) || report.vtable.length !== 47
    || report.vtable.some((value) => !RVA.test(value))) {
    throw new Error('Provider resolver returned an invalid 47-slot vtable.');
  }
  if (report.ok && (report.checks.managerClusterCandidates !== 1
    || report.checks.baseClusterCandidates !== 1
    || report.checks.pathCandidates !== 1
    || report.checks.referenceReleaseCandidates !== 1
    || report.checks.executableVtableTargets !== 47)) {
    throw new Error('Provider resolver reported success without unique structural validation.');
  }
  return report;
}

function cacheName(report) {
  return `${report.timestamp.slice(2)}-${report.imageSize.slice(2)}.json`;
}

async function runResolverExecutable(resolverExecutable, gameExecutable) {
  try {
    return await execFileAsync(resolverExecutable, [gameExecutable], {
      encoding: 'utf8',
      maxBuffer: 2 * 1024 * 1024,
      timeout: 30_000,
      windowsHide: true,
    });
  } catch (error) {
    if (typeof error.stdout === 'string' && error.stdout.includes('{')) {
      return { stdout: error.stdout, stderr: error.stderr || '' };
    }
    throw new Error(`Provider resolver could not run: ${error.message}`);
  }
}

async function writeVerifiedCache(cacheDirectory, report) {
  await fsp.mkdir(cacheDirectory, { recursive: true });
  const cachePath = path.join(cacheDirectory, cacheName(report));
  const cached = {
    ...report,
    verifiedAt: new Date().toISOString(),
    source: 'automatic-local-scan',
  };
  const temporary=`${cachePath}.${process.pid}.${Date.now()}.tmp`;
  try{await fsp.writeFile(temporary,`${JSON.stringify(cached, null, 2)}\n`,{encoding:'utf8',flag:'wx'});await fsp.rename(temporary,cachePath);}
  finally{await fsp.rm(temporary,{force:true}).catch(()=>{});}
  return { ...cached, cachePath };
}

async function readVerifiedCache(cacheDirectory,gameExecutable,gameStat){
  let entries;try{entries=await fsp.readdir(cacheDirectory,{withFileTypes:true});}catch(error){if(error.code==='ENOENT')return null;throw error;}
  const expectedPath=path.resolve(gameExecutable).toLowerCase(),expectedModified=gameStat.mtime.toISOString();
  const candidates=[];
  for(const entry of entries){if(!entry.isFile()||!/^[A-F0-9]{8}-[A-F0-9]{8}\.json$/i.test(entry.name))continue;const file=path.join(cacheDirectory,entry.name);const stat=await fsp.stat(file);candidates.push({file,modified:stat.mtimeMs});}
  candidates.sort((a,b)=>b.modified-a.modified);
  for(const candidate of candidates){
    try{
      const cached=validateResolverReport(JSON.parse(await fsp.readFile(candidate.file,'utf8')));
      if(!cached.ok||path.resolve(cached.gameExecutable||'').toLowerCase()!==expectedPath||cached.fileSize!==gameStat.size||cached.fileModifiedAt!==expectedModified)continue;
      return {...cached,source:'verified-cache',cachePath:candidate.file};
    }catch{}
  }
  return null;
}

async function resolveProviderLayout({
  resolverExecutable,
  gameExecutable,
  cacheDirectory,
  runner = runResolverExecutable,
  useCache = true,
}) {
  const resolverStat = await fsp.stat(resolverExecutable).catch(() => null);
  if (!resolverStat?.isFile()) throw new Error('The bundled provider-layout resolver is missing.');
  const gameStat = await fsp.stat(gameExecutable).catch(() => null);
  if (!gameStat?.isFile()) throw new Error('NBA2K27.exe was not found at the configured game path.');
  if(useCache){const cached=await readVerifiedCache(cacheDirectory,gameExecutable,gameStat);if(cached)return cached;}
  const { stdout, stderr } = await runner(resolverExecutable, gameExecutable);
  const report = validateResolverReport(parseResolverOutput(stdout));
  const enriched = {
    ...report,
    gameExecutable,
    fileSize: gameStat.size,
    fileModifiedAt: gameStat.mtime.toISOString(),
    resolverStderr: String(stderr || '').trim(),
  };
  if (!report.ok) return enriched;
  return writeVerifiedCache(cacheDirectory, enriched);
}

module.exports = {
  SCHEMA,
  cacheName,
  parseResolverOutput,
  readVerifiedCache,
  resolveProviderLayout,
  validateResolverReport,
};
