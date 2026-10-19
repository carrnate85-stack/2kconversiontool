'use strict';
// Only cold conversions enter this queue. Disk/memory hits bypass it entirely.
class PreviewQueue {
  constructor(limit=2) { this.limit=limit; this.active=0; this.foreground=[]; this.background=[]; }
  run(task, background=false) {
    return new Promise((resolve,reject)=>{
      (background?this.background:this.foreground).push({task,resolve,reject});this.drain();
    });
  }
  drain() {
    while(this.active<this.limit && (this.foreground.length||this.background.length)) {
      const job=(this.foreground.length?this.foreground:this.background).shift();this.active++;
      Promise.resolve().then(job.task).then(job.resolve,job.reject).finally(()=>{this.active--;this.drain();});
    }
  }
}
module.exports={PreviewQueue};
