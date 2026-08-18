// The imports a bundler resolves and a type checker does not: a single-file
// component and a stylesheet are not JavaScript, and `types:tsc` is checking
// the JavaScript. Typing them as `any` says "not this gate's business" rather
// than pretending to know their shape — `.vue` files are covered by the tests
// and by eslint, and a stylesheet has no shape to check.
declare module '*.vue';
declare module '*.css';
