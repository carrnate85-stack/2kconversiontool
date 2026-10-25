// Appended to the retained viewer scope; uses the existing camera and draw().
function chooseView(view) {
 stopPan();drag=null;
 if(view==='reset'){yaw=0.45;pitch=0.1;fit();}
 else {yaw=view==='back'?Math.PI:0;pitch=0;draw();}
}
document.querySelectorAll('.studio-controls [data-view]').forEach(button=>{
 button.addEventListener('click',()=>chooseView(button.dataset.view));
});
document.querySelectorAll('.studio-controls [data-backdrop]').forEach(button=>{
 button.addEventListener('click',()=>{
  document.body.dataset.backdrop=button.dataset.backdrop;
  document.querySelectorAll('.studio-controls [data-backdrop]').forEach(choice=>choice.setAttribute('aria-pressed',String(choice===button)));
 });
});
canvas.addEventListener('dblclick',event=>{event.preventDefault();stopPan();drag=null;fit();});
canvas.setAttribute('aria-label','3D clothing mannequin. Drag to orbit, scroll to zoom, double-click to fit, hold W or S to pan vertically.');
