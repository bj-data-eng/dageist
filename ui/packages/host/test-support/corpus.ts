/**
 * @file Test support: reads the files of the corpus, `corpus-*.json`, which the host's tests write:
 * step documents the engine made by its own edits, each with the report the host gives for it, and
 * the inputs the corpus's documents are run with. It is not something the host sends, so it is not
 * part of the package's front door. A document and its report are read by the decoders the view
 * reads the host with, so that one the contract refuses is refused here, with its place.
 */

import type { DocumentView, InputView, RunView } from '../src/contract';
import type { Decode } from '../src/decode';
import { ROOT, readField, readList, readRecord, readString } from '../src/decode';
import { readDocument } from '../src/decode-document';
import { readInput } from '../src/decode-node';
import { readRun } from '../src/decode-run';

/** One document of the corpus, with a name that says what it is for, and what running it reported. */
export interface CorpusDocument {
  readonly name: string;
  readonly document: DocumentView;
  readonly run: RunView;
}

/** One file of the corpus. */
export interface CorpusPart {
  /** What is connected to the node the documents are run on: the single values and lists their chains may start from. */
  readonly inputs: readonly InputView[];
  readonly documents: readonly CorpusDocument[];
}

const readCorpusDocument: Decode<CorpusDocument> = readRecord(
  ['name', 'document', 'run'],
  (fields, path) => ({
    name: readField(fields, path, 'name', readString),
    document: readField(fields, path, 'document', readDocument),
    run: readField(fields, path, 'run', readRun),
  }),
);

const readPart: Decode<CorpusPart> = readRecord(['inputs', 'documents'], (fields, path) => ({
  inputs: readField(fields, path, 'inputs', readList(readInput)),
  documents: readField(fields, path, 'documents', readList(readCorpusDocument)),
}));

/**
 * Reads a file of the corpus the host wrote.
 *
 * @throws {DecodeError} When it is not one, saying where and why.
 */
export function decodeCorpusPart(value: unknown): CorpusPart {
  return readPart(value, ROOT);
}
