export type MeasurementInput={value:string;dataset:Record<string,string>};
export const playerMeasurements:{
  inches:(cm?:number)=>number|null;
  parts:(cm?:number)=>{feet:number;inches:number}|null;
  writeHeight:(feet:MeasurementInput,inches:MeasurementInput,cm?:number)=>void;
  readHeight:(feet:MeasurementInput,inches:MeasurementInput)=>number;
  writeWingspan:(input:MeasurementInput,cm?:number)=>void;
  readWingspan:(input:MeasurementInput)=>number;
};
export const colorHex:(value:number)=>string;
export const argbToInput:(value?:number)=>string;
export const inputToArgb:(value:string,original:number)=>number;
