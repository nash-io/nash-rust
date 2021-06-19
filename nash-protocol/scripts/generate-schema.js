'use strict';

const https = require('https');
const gql = require('graphql/utilities');


const abort = err => {
  process.stderr.write (err.message + '\n');
  process.exit (1);
};

const opts = {
  method: 'POST',
  headers: {
    'Accept': 'application/json',
    'Content-Type': 'application/json',
  },
};

let backend = process.env.BACKEND || "https://app.nash.io/api/graphql";

https.request(backend, opts, res => {
  let body = '';
  res.setEncoding ('utf8')
  .on ('data', chunk => { body += chunk; })
  .on ('end', () => {
    try {
      process.stdout.write (
        JSON.stringify((JSON.parse(body)).data, null, 2)
      );
    } catch (err) {
      abort (err);
    }
  });
})
.on ('error', abort)
.end (JSON.stringify ({query: gql.introspectionQuery}));


