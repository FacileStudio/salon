/** What a connection chip shows: syncing or reconnecting, live, offline, or stopped by the server. */
export type Connection = 'connecting' | 'live' | 'offline' | 'stopped';

/** Why the server ended the session for good, from its close code. */
export type Stop = 'refused' | 'too-large' | 'failed';

/** The y-websocket message type of the server's custom notices. */
export const MESSAGE_CUSTOM = 100;

const STOPS: Record<number, Stop> = { 1008: 'refused', 1009: 'too-large', 1011: 'failed' };

/**
 * The stop a close code means, or null when the client should reconnect with backoff: 1001 (room closed),
 * 1013 (too slow or restarting) and network drops reconnect; 1008 (refused change or access), 1009 (too
 * large) and 1011 (room failed) stop.
 */
export function stopOf(code: number): Stop | null {
	return STOPS[code] ?? null;
}

/** The connection state from the provider status, the first sync and the browser's network flag. */
export function connectionOf(status: string, synced: boolean, online: boolean): Connection {
	if (!online) return 'offline';
	return status === 'connected' && synced ? 'live' : 'connecting';
}

/** The WebSocket URL of `path` on `location`'s host: `wss:` on an https page, `ws:` otherwise. */
export function socketUrl(path: string, location: Pick<Location, 'protocol' | 'host'>): string {
	const scheme = location.protocol === 'https:' ? 'wss:' : 'ws:';
	return `${scheme}//${location.host}${path}`;
}
