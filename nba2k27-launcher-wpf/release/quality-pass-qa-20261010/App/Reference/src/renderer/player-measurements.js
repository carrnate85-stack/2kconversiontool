/* Display units only. Game records and .2kplayer documents retain centimetres. */
(function(root,factory){if(typeof module==='object'&&module.exports)module.exports=factory();else root.playerMeasurements=factory();})(typeof window==='undefined'?globalThis:window,()=>{
 const inches=cm=>Number.isFinite(cm)?Math.round(cm/2.54*1000)/1000:null;
 function parts(cm){const total=inches(cm);return total===null?null:{feet:Math.floor(total/12),inches:Math.round((total%12)*1000)/1000};}
 function number(input,label){if(String(input.value).trim()===''||!Number.isFinite(Number(input.value)))throw Error('Enter a valid '+label+'.');return Number(input.value);}
 function bounded(cm,max,label){if(!Number.isFinite(cm)||cm<120||cm>max)throw Error(label+' is outside the supported game range.');return cm;}
 function writeHeight(feetInput,inchInput,cm){const value=parts(cm);feetInput.value=value?String(value.feet):'';inchInput.value=value?String(value.inches):'';feetInput.dataset.measurementCm=value?String(cm):'';feetInput.dataset.measurementFeet=feetInput.value;feetInput.dataset.measurementInches=inchInput.value;}
 function readHeight(feetInput,inchInput){const feet=number(feetInput,'height in feet'),extra=number(inchInput,'height in inches');if(!Number.isInteger(feet)||feet<0||extra<0||extra>=12)throw Error('Enter whole feet and inches from 0 to less than 12.');const same=feetInput.dataset.measurementCm!==''&&feet===Number(feetInput.dataset.measurementFeet)&&extra===Number(feetInput.dataset.measurementInches);return bounded(same?Number(feetInput.dataset.measurementCm):Math.round((feet*12+extra)*254)/100,260,'Height');}
 function writeWingspan(input,cm){const value=inches(cm);input.value=value===null?'':String(value);input.dataset.measurementCm=value===null?'':String(cm);input.dataset.measurementInches=input.value;}
 function readWingspan(input){const value=number(input,'wingspan in inches'),same=input.dataset.measurementCm!==''&&value===Number(input.dataset.measurementInches);return bounded(same?Number(input.dataset.measurementCm):Math.round(value*254)/100,320,'Wingspan');}
 return {inches,parts,writeHeight,readHeight,writeWingspan,readWingspan};
});
