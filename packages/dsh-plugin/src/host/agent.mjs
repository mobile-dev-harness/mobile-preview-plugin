import { ServiceError } from './service.mjs';

const cancelled = () => new ServiceError('ABORTED', 'The mobile preview request was cancelled.',
  'Retry from the current conversation.');

/** Attach the conversation-bound tool only while DSH provides its tool registry. */
export function registerAgentTool(ctx, service) {
  ctx.inject(['tools'], (toolCtx) => {
    toolCtx.tools.register({
      name: 'open_mobile_preview',
      description: 'Request the mobile preview panel for this conversation once the mobile project platform is known. '
        + 'Choose android or ios. Android supports device control; iOS Simulator on macOS supports touch and Home when available, with a read-only fallback. '
        + 'Does not start or connect a device.',
      parameters: {
        type: 'object',
        properties: { platform: { type: 'string', enum: ['android', 'ios'] } },
        required: ['platform'], additionalProperties: false,
      },
      output: {
        schema: {
          type: 'object',
          properties: {
            sessionId: { type: 'string' },
            platform: { type: 'string', enum: ['android', 'ios'] },
            source: { type: 'string', const: 'agent' },
            revision: { type: 'integer' },
            epoch: { type: 'string' },
            available: { type: 'boolean' },
          },
          required: ['sessionId', 'platform', 'source', 'revision', 'epoch', 'available'],
          additionalProperties: false,
        },
        render: (_args, value) => [{
          type: 'text',
          text: value.platform === 'ios'
            ? value.available
              ? 'Requested the iOS Simulator preview panel for this conversation. Choose a simulator in the panel to connect; supported simulators offer touch, drag and Home, with a read-only fallback when input is unavailable.'
              : 'Selected iOS and requested the mobile preview panel for this conversation. '
              + 'The iOS backend is unavailable; this is platform selection only.'
            : 'Requested the Android mobile preview panel for this conversation. Choose a device in the panel to connect.',
        }],
      },
      async execute(args, exec) {
        if (args === null || typeof args !== 'object' || Array.isArray(args)
          || Object.keys(args).length !== 1 || !Object.hasOwn(args, 'platform')
          || !['android', 'ios'].includes(args.platform)) {
          throw new ServiceError('INVALID_ARGUMENT', 'Expected only platform: android or ios.',
            'Provide platform as android or ios without other parameters.');
        }
        const sessionId = exec?.agent?.session?.id;
        if (typeof sessionId !== 'string' || !sessionId) {
          throw new ServiceError('SESSION_REQUIRED', 'Mobile preview requires a current conversation.',
            'Call this tool from the conversation that needs mobile preview.');
        }
        if (exec.signal.aborted) throw cancelled();
        try {
          return await service.requestPlatform(sessionId, args.platform, { signal: exec.signal });
        } catch (error) {
          if (exec.signal.aborted) throw cancelled();
          throw error;
        }
      },
    });
  });
}
