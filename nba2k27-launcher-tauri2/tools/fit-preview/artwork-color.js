// QA120: source LogoNColorizeR/G/B colors use the same RGB-region model as clothing.
function configureArtworkPalette(shader,prefix,palette,declarations){
 if(!palette)return '';
 const channels=['Red','Green','Blue'];
 channels.forEach((channel,i)=>{
  shader.uniforms[prefix+channel]={value:new THREE.Color().setRGB(...palette[i].map(v=>v/255),THREE.SRGBColorSpace)};
  declarations.push(`uniform vec3 ${prefix}${channel};`);
 });
 return `artwork.rgb=artwork.r*${prefix}Red+artwork.g*${prefix}Green+artwork.b*${prefix}Blue;`;
}
