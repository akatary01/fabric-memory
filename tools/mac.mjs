// Compatibility entry; fabric-memory owns all app startup.
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
const cli=fileURLToPath(new URL('../bin/fabric-memory',import.meta.url));
const result=spawnSync(cli,['run',...process.argv.slice(2)],{stdio:'inherit'});
if(result.error)throw result.error;
process.exitCode=result.status ?? 1;
