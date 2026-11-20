// The only module that talks to the backend. Views import from here; they never call `invoke`.
import { invoke } from '@tauri-apps/api/core';
import type { Attempt } from '#lib/api/types/Attempt';
import type { BackupReport } from '#lib/api/types/BackupReport';
import type { AttemptView } from '#lib/api/types/AttemptView';
import type { Card } from '#lib/api/types/Card';
import type { CardFilter } from '#lib/api/types/CardFilter';
import type { CardState } from '#lib/api/types/CardState';
import type { CardSummary } from '#lib/api/types/CardSummary';
import type { Category } from '#lib/api/types/Category';
import type { ConflictView } from '#lib/api/types/ConflictView';
import type { CleanupReport } from '#lib/api/types/CleanupReport';
import type { Grade } from '#lib/api/types/Grade';
import type { MediaInfo } from '#lib/api/types/MediaInfo';
import type { MergeReport } from '#lib/api/types/MergeReport';
import type { PackPreview } from '#lib/api/types/PackPreview';
import type { PackReport } from '#lib/api/types/PackReport';
import type { SaveResult } from '#lib/api/types/SaveResult';
import type { SchedConfig } from '#lib/api/types/SchedConfig';
import type { SessionPlan } from '#lib/api/types/SessionPlan';
import type { Settings } from '#lib/api/types/Settings';
import type { StatsOverview } from '#lib/api/types/StatsOverview';
import type { StepStats } from '#lib/api/types/StepStats';
import type { Summary } from '#lib/api/types/Summary';
import type { SyncView } from '#lib/api/types/SyncView';

export type { Attempt, AttemptView, Card, CardFilter, CardState, CardSummary, Category };
export type {
	Grade,
	MediaInfo,
	SaveResult,
	SchedConfig,
	SessionPlan,
	Settings,
	StepStats,
	Summary
};

export type {
	BackupReport,
	CleanupReport,
	ConflictView,
	MergeReport,
	PackPreview,
	PackReport,
	StatsOverview,
	SyncView
};

export const api = {
	/** Base URL of the loopback media server; append `<hh>/<hash>.<ext>` (SPEC §10). */
	mediaBaseUrl: () => invoke<string>('media_base_url'),

	categoriesList: () => invoke<Category[]>('categories_list'),
	categorySave: (category: Category) => invoke<Category>('category_save', { category }),
	categoryDelete: (id: string) => invoke<void>('category_delete', { id }),

	cardsQuery: (filter: Partial<CardFilter> = {}) =>
		invoke<CardSummary[]>('cards_query', { filter: { ...emptyFilter, ...filter } }),
	cardGet: (id: string) => invoke<Card | null>('card_get', { id }),
	/** `remember` (the Add dialog only) seeds the next new card with this card's fields. */
	cardSave: (card: Card, remember = false) => invoke<SaveResult>('card_save', { card, remember }),
	cardWarnings: (card: Card) => invoke<string[]>('card_warnings', { card }),
	cardDelete: (id: string) => invoke<void>('card_delete', { id }),
	cardSetState: (id: string, cardState: CardState) =>
		invoke<void>('card_set_state', { id, cardState }),
	/** Rename (or merge into an existing tag) on every card; returns cards changed. */
	tagRename: (from: string, to: string) => invoke<number>('tag_rename', { from, to }),
	tagDelete: (tag: string) => invoke<number>('tag_delete', { tag }),

	/** Raw bytes travel as the request body, not JSON. */
	mediaImportBytes: (bytes: Uint8Array) => invoke<MediaInfo>('media_import_bytes', bytes),
	mediaImportPath: (path: string) => invoke<MediaInfo>('media_import_path', { path }),
	/** Image on the OS clipboard (read once, on an explicit paste), or null. */
	mediaImportClipboard: () => invoke<MediaInfo | null>('media_import_clipboard'),
	mediaCleanup: () => invoke<CleanupReport>('media_cleanup'),

	backupExport: (path: string) => invoke<BackupReport>('backup_export', { path }),
	backupImport: (path: string) => invoke<MergeReport>('backup_import', { path }),
	packExport: (path: string, cardIds: string[], name: string) =>
		invoke<number>('pack_export', { path, cardIds, name }),
	syncStatus: () => invoke<SyncView>('sync_status'),
	/** Starts a background sync; false if one is already running. */
	syncNow: () => invoke<boolean>('sync_now'),
	conflictsList: () => invoke<ConflictView[]>('conflicts_list'),
	conflictRestore: (id: string) => invoke<void>('conflict_restore', { id }),
	conflictDismiss: (id: string) => invoke<void>('conflict_dismiss', { id }),
	packInspect: (path: string) => invoke<PackPreview>('pack_inspect', { path }),
	packImport: (path: string, update: string[]) =>
		invoke<PackReport>('pack_import', { path, update }),

	statsOverview: () => invoke<StatsOverview>('stats_overview'),

	settingsGet: () => invoke<Settings>('settings_get'),
	settingsSet: (settings: Settings) => invoke<void>('settings_set', { settings }),
	schedConfigGet: () => invoke<SchedConfig>('sched_config_get'),
	schedConfigSet: (config: SchedConfig) => invoke<void>('sched_config_set', { config }),
	/** Intervals (days) after successive good reviews at this retention. */
	schedPreview: (retention: number) => invoke<number[]>('sched_preview', { retention }),

	sessionBuild: (includeAll = false) => invoke<SessionPlan>('session_build', { includeAll }),
	sessionCram: (cardIds: string[]) => invoke<SessionPlan>('session_cram', { cardIds }),
	attemptStart: (cardId: string, cram = false) =>
		invoke<AttemptView>('attempt_start', { cardId, cram }),
	attemptGet: (attemptId: string) => invoke<AttemptView>('attempt_get', { attemptId }),
	attemptStepShow: (attemptId: string, stepId: string) =>
		invoke<AttemptView>('attempt_step_show', { attemptId, stepId }),
	attemptStepReveal: (attemptId: string, stepId: string, typedAnswer: string | null) =>
		invoke<AttemptView>('attempt_step_reveal', { attemptId, stepId, typedAnswer }),
	/** The revealed step is done; ticked checklist items are kept for Stats only. */
	attemptStepComplete: (attemptId: string, stepId: string, checked: string[]) =>
		invoke<AttemptView>('attempt_step_complete', { attemptId, stepId, checked }),
	/** `draft`: the typed answer on screen, restored on Resume. */
	attemptPause: (attemptId: string, draft: string | null) =>
		invoke<AttemptView>('attempt_pause', { attemptId, draft }),
	attemptDiscard: (attemptId: string) => invoke<void>('attempt_discard', { attemptId }),
	attemptRestart: (attemptId: string) => invoke<AttemptView>('attempt_restart', { attemptId }),
	/** The user's own grade for the card: the only thing that schedules it. */
	attemptFinish: (attemptId: string, grade: Grade) =>
		invoke<Summary>('attempt_finish', { attemptId, grade }),
	/** Days until the card returns for Forgot, Hard, Good, Easy. */
	gradePreview: (cardId: string) => invoke<number[]>('grade_preview', { cardId }),
	drillRecord: (cardId: string, stepId: string, grade: Grade, elapsedMs: number) =>
		invoke<StepStats>('drill_record', { cardId, stepId, grade, elapsedMs })
};

const emptyFilter: CardFilter = {
	text: null,
	category_id: null,
	state: null,
	due_only: false,
	flagged_only: false,
	today: null
};

/** Backend errors arrive as `{ message }`; anything else is stringified. */
export function errorMessage(e: unknown): string {
	if (e && typeof e === 'object' && 'message' in e)
		return String((e as { message: unknown }).message);
	return String(e);
}
