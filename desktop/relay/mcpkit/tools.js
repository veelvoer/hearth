'use strict';
/* Your tools. Each one: { name, description, inputSchema, run(args, { exec, http, env }), sample, sampleError }.
   - name: lowercase_with_underscores. description: one or two clear sentences that tell Claude WHEN to use it.
   - inputSchema: JSON Schema of the arguments, with "required" and a description for every property.
   - run: returns a string or an object. Throw an Error with a helpful message when something is wrong.
   - sample: example arguments that are SAFE to run in a test (read-only). check.js calls every tool that has one.
   - sampleError: true when the sample is expected to give an error (for example a missing program).
   Delete the example below and add the real tools. */
module.exports = [
  {
    name: 'echo',
    description: 'Gives back the text it receives. Only an example: delete it.',
    inputSchema: { type: 'object', properties: { text: { type: 'string', description: 'The text to repeat.' } }, required: ['text'] },
    annotations: { readOnlyHint: true },
    sample: { text: 'hello' },
    run: async ({ text }) => text,
  },
];
