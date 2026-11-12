// In-memory sample data for the browser preview. No native bridge is connected.
const previewTitle = 'UI preview · sample data';
const previewSteps = [
  { kind: 'running', tool: 'run_code', summary: 'Running sample workspace check…', command: 'python3 check.py' },
  { kind: 'done', tool: 'run_code', summary: 'Sample check returned an error',
    result: { exit_code: 1, stderr: 'Example: input.csv was not found.' } },
].map(step => ({ ...step, time: Date.now() / 1000 }));
const previewConversation = {
  id: 'preview',
  title: previewTitle,
  model: 'Preview model',
  turns: [
    { role: 'user', content: 'Run the workspace check.' },
    { role: 'assistant', content: 'The sample check failed because the input file is missing.', steps: previewSteps },
  ],
  conversations: [{ id: 'preview', title: previewTitle }],
  deleted: [],
};

const previewState = {
  conversation: previewConversation,
  status: { events: 0, automatic_sync: 'Browser preview', beacon_connected: true },
  memory_view: { live_memories: 0, memories: [] },
  agent_status: { running: false, steps: [] },
  agent_history: previewSteps,
  running_tasks: {},
  setup_state: { complete: true },
  device_usage: { devices: [] },
  beacon_settings: { url: '', name: 'Browser preview' },
  local_beacon_status: { running: false },
  mcp_catalog: [],
  workspace_settings: { path: '/example/workspace' },
  agent_settings: { task_budget_minutes: 5, auto_approve: false },
  cloud_settings: {
    provider_id: 'preview', providers: [{ id: 'preview', name: 'Preview provider' }],
    model: 'Preview model', base_url: '', automatic: false, enabled: false,
  },
  model_status: { model: 'Preview model', status: 'Ready' },
};

window.__TAURI__ = { core: { async invoke(_command, { command, args }) {
  if (Object.hasOwn(previewState, command)) return structuredClone(previewState[command]);
  if (command === 'clipboard_write') return navigator.clipboard.writeText(args.text);
  if (command === 'send_prompt') {
    previewConversation.turns.push(
      { role: 'user', content: args.text },
      { role: 'assistant', content: 'This browser preview uses sample data. Open the Mac app to run models and tools.', steps: previewSteps },
    );
    return {};
  }
  throw new Error('This action is available in the native Mac app.');
} } };
