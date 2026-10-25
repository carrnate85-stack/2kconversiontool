const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { Rpc, root, currentExe } = require('./rpc-client.cjs');
const digest = p => crypto.createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const snapshot = dir => Object.fromEntries(fs.readdirSync(dir).filter(n=>/\.iff$/i.test(n)).map(n=>[n,digest(path.join(dir,n))]));
(async()=>{
  const version = JSON.parse(fs.readFileSync(path.join(root,'dist/BUILD.json'),'utf8').replace(/^\uFEFF/,'')).version;
  const folder=fs.mkdtempSync(path.join(root,'artifacts/direct-stage-'));
  const data=path.join(folder,'data'); fs.mkdirSync(data);
  let rpc=new Rpc({env:{CMT_TEST_DATA:data}});
  const checks=[], stagedPaths=[];
  try {
    await rpc.call('bootstrap');
    await rpc.call('save-settings',{game_root_2k27:'C:/Program Files (x86)/Steam/steamapps/common/NBA 2K27',
      output_dir:path.join(folder,'output')});
    const stock=await rpc.call('extract',{gameLabel:'NBA 2K27',png:'06827'});
    const converted=path.join(root,'artifacts/automatic-seams-1buCO1/output/png06827.iff');
    for (const [kind,source,gameLabel] of [['stock',stock.mainPath,'NBA 2K27'],['converted',converted,'']]) {
      assert.ok(fs.existsSync(source),source);
      const original=snapshot(path.dirname(source));
      const started=Date.now();
      const staged=await rpc.call('stage-player',{filePath:source,gameLabel});
      assert.notEqual(path.resolve(staged.mainPath),path.resolve(source));
      assert.ok(staged.mainPath.includes('Edited Players'));
      const configs=await rpc.call('configs-list',{target:staged.mainPath});
      assert.ok(configs.configs.length>0,'No editable configurations');
      const body=await rpc.call('body-fit',{target:staged.mainPath});
      const entry=Object.entries(body.targetBodyFit).find(([key,value])=>typeof value==='number' && Number.isFinite(value));
      assert.ok(entry,'No editable body fit');
      const before=digest(staged.mainPath);
      await rpc.call('body-fit-save',{target:staged.mainPath,changes:{[entry[0]]:entry[1]+0.01}});
      assert.notEqual(digest(staged.mainPath),before,'Body edit did not reach staged copy');
      const updated=await rpc.call('body-fit',{target:staged.mainPath});
      assert.ok(Math.abs(updated.targetBodyFit[entry[0]]-entry[1]-0.01)<1e-5);
      const preview=await rpc.call('native-preview-resources',{target:staged.mainPath});
      assert.ok(preview && preview.filePath,'Native preview did not resolve');
      const hair=await rpc.call('native-hair-target',{target:staged.mainPath});
      const headbands=await rpc.call('headband-options',{target:staged.mainPath});
      assert.ok(hair && headbands,'Accessory editors did not resolve');
      assert.deepEqual(snapshot(path.dirname(source)),original,'Source files changed');
      const finalHash=digest(staged.mainPath);
      await rpc.call('reset');
      assert.equal(digest(staged.mainPath),finalHash,'Reset removed durable copy');
      stagedPaths.push([staged.mainPath,finalHash]);
      checks.push({kind,elapsedMs:Date.now()-started,companions:staged.companions,warnings:staged.warnings,
        configs:configs.configs.length,bodyEdit:true,nativePreview:true,accessoryEditors:true,sourceUnchanged:true,resetDurable:true});
    }
    await rpc.close(); rpc=new Rpc({env:{CMT_TEST_DATA:data}});
    await rpc.call('bootstrap');
    for(const [target,hash] of stagedPaths) {
      assert.equal(digest(target),hash);
      assert.ok((await rpc.call('configs-list',{target})).configs.length);
    }
    const report={version,executableSha256:digest(currentExe()),headless:true,desktopUntouched:true,restartDurable:true,checks};
    fs.writeFileSync(path.join(root,`artifacts/direct-stage-${version}.json`),JSON.stringify(report,null,2));
    console.log(JSON.stringify(report,null,2));
  } finally { await rpc.close(); }
})().catch(error=>{console.error(error);process.exitCode=1;});

