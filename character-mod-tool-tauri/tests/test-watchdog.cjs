module.exports = function testWatchdog(label, milliseconds = 30000) {
  const timer = setTimeout(() => {
    console.error('Test did not finish: ' + label);
    process.exit(1);
  }, milliseconds);
  return () => clearTimeout(timer);
};
