import * as encoding from 'lib0/encoding';
import * as decoding from 'lib0/decoding';
import { describe, expect, it, vi } from 'vitest';
import * as Y from 'yjs';
import { connectionOf, MESSAGE_CUSTOM, openRoom, socketUrl, stopOf, type Connection } from './index';

type Listener = (event: never) => void;

const opened = vi.hoisted(() => [] as FakeProvider[]);

interface FakeProvider {
	url: string;
	room: string;
	options: { shouldReconnect: (event: { code: number }) => boolean; params: Record<string, string> };
	synced: boolean;
	destroyed: boolean;
	messageHandlers: Record<number, (encoder: unknown, decoder: decoding.Decoder) => void>;
	emit: (name: string, event: unknown) => void;
}

vi.mock('y-websocket', () => ({
	WebsocketProvider: class {
		synced = false;
		destroyed = false;
		messageHandlers: Record<number, unknown> = {};
		private listeners: Record<string, Listener[]> = {};
		constructor(
			public url: string,
			public room: string,
			_doc: unknown,
			public options: unknown
		) {
			opened.push(this as unknown as FakeProvider);
		}
		on(name: string, listener: Listener) {
			(this.listeners[name] ??= []).push(listener);
		}
		emit(name: string, event: unknown) {
			for (const listener of this.listeners[name] ?? []) listener(event as never);
		}
		destroy() {
			this.destroyed = true;
		}
	}
}));

function notice(text: string): decoding.Decoder {
	const encoder = encoding.createEncoder();
	encoding.writeVarString(encoder, text);
	return decoding.createDecoder(encoding.toUint8Array(encoder));
}

describe('codes', () => {
	it('builds the socket URL of the page origin', () => {
		expect(socketUrl('/api/documents', { protocol: 'https:', host: 'docs.facile.studio' })).toBe(
			'wss://docs.facile.studio/api/documents'
		);
		expect(socketUrl('/api/boards', { protocol: 'http:', host: 'localhost:5173' })).toBe('ws://localhost:5173/api/boards');
	});

	it('shows live only once connected and synced, offline whenever the browser is', () => {
		expect(connectionOf('connected', true, true)).toBe('live');
		expect(connectionOf('connected', false, true)).toBe('connecting');
		expect(connectionOf('disconnected', false, true)).toBe('connecting');
		expect(connectionOf('connected', true, false)).toBe('offline');
	});

	it('stops on refused, too large and failed rooms, and reconnects on the others', () => {
		expect([stopOf(1008), stopOf(1009), stopOf(1011)]).toEqual(['refused', 'too-large', 'failed']);
		expect([stopOf(1001), stopOf(1013), stopOf(1006)]).toEqual([null, null, null]);
	});
});

describe('openRoom', () => {
	it('reports the connection, relays custom notices, and stops for good on a terminal close', () => {
		const states: Connection[] = [];
		const notices: unknown[] = [];
		const stops: [string, string][] = [];
		const events = {
			onconnection: (state: Connection) => states.push(state),
			onstop: (stop: string, reason: string) => stops.push([stop, reason]),
			oncustom: (value: unknown) => notices.push(value),
			onroomclose: vi.fn()
		};
		const room = openRoom({ base: 'ws://h/api/boards', room: 'b1/ws', params: { name: 'Ana' } }, new Y.Doc(), events);
		const provider = opened.at(-1)!;
		expect([provider.url, provider.room, provider.options.params]).toEqual(['ws://h/api/boards', 'b1/ws', { name: 'Ana' }]);
		expect(provider.options.shouldReconnect({ code: 1013 })).toBe(true);
		expect(provider.options.shouldReconnect({ code: 1008 })).toBe(false);
		provider.messageHandlers[MESSAGE_CUSTOM](null, notice('{"type":"comments-changed"}'));
		provider.messageHandlers[MESSAGE_CUSTOM](null, notice('not json'));
		expect(notices).toEqual([{ type: 'comments-changed' }, null]);
		provider.synced = true;
		provider.emit('status', { status: 'connected' });
		provider.emit('connection-close', { code: 1001 });
		provider.emit('closed', { code: 1008, reason: 'access revoked' });
		expect(states.at(-2)).toBe('live');
		expect(states.at(-1)).toBe('stopped');
		expect(stops).toEqual([['refused', 'access revoked']]);
		expect(events.onroomclose).toHaveBeenCalledOnce();
		room.destroy();
		expect(provider.destroyed).toBe(true);
	});
});
