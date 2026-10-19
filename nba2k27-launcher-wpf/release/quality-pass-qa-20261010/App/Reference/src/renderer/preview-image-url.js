(() => {
  const kinds=new Set(['top','bottom','accessory','ball','sock','shoe']);
  let revision=0;
  window.PreviewImageUrl=(kind,relative)=>{
    if(!kinds.has(kind)||typeof relative!=='string')return 'about:blank';
    const [file,query='']=relative.split('?',2);
    if(!/^thumbnails\/[a-zA-Z0-9_.-]+\.png$/.test(file)||file.includes('..'))return 'about:blank';
    const prefix=window.__wpfRenderer?'https://launcher-resources.invalid/__preview/':'preview-image://images/';
    return `${prefix}blacktop-${kind}-previews/${file}${query?`?${query}`:''}${revision?`${query?'&':'?'}pack=${revision}`:''}`;
  };
  window.PreviewImageUrl.refresh=()=>{revision++;};
})();
