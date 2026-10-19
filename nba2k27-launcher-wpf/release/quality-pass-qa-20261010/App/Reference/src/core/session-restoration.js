'use strict';

async function runRestorationSteps(steps, {beforeEach=async()=>{}}={}) {
  const errors=[];
  for(const [name,restore] of steps) {
    // Safety gates abort the batch, unlike independent restoration failures.
    // Never stop later helpers after a fresh check says the game is still live.
    await beforeEach(name);
    try { await restore(); }
    catch(cause) { errors.push(new Error(`${name}: ${cause.message || cause}`,{cause})); }
  }
  if(errors.length)throw new AggregateError(errors,errors.map(error=>error.message).join('; '));
}

module.exports={runRestorationSteps};
