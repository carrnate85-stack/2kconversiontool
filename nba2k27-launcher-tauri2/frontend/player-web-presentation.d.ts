export type PresentationField = {key:string;group:string;money?:boolean};
export const playerEditorLayout: {
  categories: Record<string,string[]>;
  category: (field:PresentationField) => string;
  order: (field:PresentationField) => number;
  personalityPairs: {label:string;keys:string[];choices:string[]}[];
};
export const playerDisplay: (field:Pick<PresentationField,"group"|"money">,raw:number) => number;
export const playerRaw: (field:Pick<PresentationField,"group"|"money">,value:number) => number;
