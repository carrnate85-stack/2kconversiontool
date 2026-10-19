'use strict';
const {buildLabels}=require('./shoe-style-names');
module.exports=buildLabels(require('../data/streetwear-components.json'),require('../data/shoe-model-names.json').models);
