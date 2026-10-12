import { useCallback, useEffect, useState } from 'react';
import { address, isAddress, type Address, type Instruction } from '@solana/kit';
import type { Wallet } from '@wallet-standard/base';
import {
  CLUSTER,
  DEFAULT_MINT,
  PROGRAM_ID,
  buildTransaction,
  createJarInstruction,
  fetchJar,
  fetchMint,
  fetchTokenBalance,
  formatAmount,
  maxTipRaw,
  parseAmount,
  simulate,
  tipInstruction,
  waitForConfirmation,
  withdrawInstructions,
  MAX_TIP_TOKENS,
  type Jar,
} from './client';
import { connectWallet, disconnectWallet, listWallets, onWalletsChanged, signAndSend, type Connected } from './wallet';

type Lookup = { owner: Address; mint: Address; jar: Jar | null };
type Notice = { kind: 'ok' | 'err' | 'info'; text: string; sig?: string };

const short = (a: string) => `${a.slice(0, 4)}…${a.slice(-4)}`;
const explorer = (sig: string) => `https://solscan.io/tx/${sig}`;

function hashOwner(): string {
  const m = window.location.hash.match(/^#\/([1-9A-HJ-NP-Za-km-z]{32,44})$/);
  return m ? m[1] : '';
}

export function App() {
  const [wallets, setWallets] = useState<Wallet[]>(() => listWallets());
  const [picker, setPicker] = useState(false);
  const [conn, setConn] = useState<Connected | null>(null);
  const [ownerInput, setOwnerInput] = useState(hashOwner());
  const [mintInput, setMintInput] = useState<string>(DEFAULT_MINT);
  const [lookup, setLookup] = useState<Lookup | null>(null);
  const [tipText, setTipText] = useState('');
  const [myBalance, setMyBalance] = useState<bigint | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [notice, setNotice] = useState<Notice | null>(null);

  useEffect(() => onWalletsChanged(() => setWallets(listWallets())), []);

  const doLookup = useCallback(async (ownerText: string, mintText: string) => {
    if (!isAddress(ownerText.trim())) return setNotice({ kind: 'err', text: 'Enter a valid owner address.' });
    if (!isAddress(mintText.trim())) return setNotice({ kind: 'err', text: 'Enter a valid token mint address.' });
    const owner = address(ownerText.trim());
    const mint = address(mintText.trim());
    setBusy('lookup');
    setNotice(null);
    try {
      const jar = await fetchJar(owner, mint);
      setLookup({ owner, mint, jar });
      window.location.hash = `#/${owner}`;
    } catch (e) {
      setNotice({ kind: 'err', text: errText(e) });
    } finally {
      setBusy(null);
    }
  }, []);

  useEffect(() => {
    if (hashOwner()) void doLookup(hashOwner(), DEFAULT_MINT);
  }, [doLookup]);

  const jar = lookup?.jar ?? null;

  useEffect(() => {
    setMyBalance(null);
    if (!conn || !jar) return;
    fetchTokenBalance(conn.address, jar.mint, jar.tokenProgram).then(setMyBalance, () => setMyBalance(null));
  }, [conn, jar]);

  const connect = async (w: Wallet) => {
    setPicker(false);
    setBusy('connect');
    try {
      const c = await connectWallet(w);
      setConn(c);
      if (!ownerInput) setOwnerInput(c.address);
      setNotice(null);
    } catch (e) {
      setNotice({ kind: 'err', text: errText(e) });
    } finally {
      setBusy(null);
    }
  };

  const disconnect = async () => {
    if (conn) await disconnectWallet(conn.wallet).catch(() => undefined);
    setConn(null);
  };

  const send = async (label: string, instructions: Instruction[], okText: string) => {
    if (!conn || !lookup) return;
    setBusy(label);
    setNotice({ kind: 'info', text: 'Preparing transaction…' });
    try {
      const { bytes } = await buildTransaction(conn.address, instructions);
      await simulate(bytes);
      setNotice({ kind: 'info', text: 'Approve the transaction in your wallet…' });
      const sigBytes = await signAndSend(conn, bytes);
      setNotice({ kind: 'info', text: 'Waiting for confirmation…' });
      const sig = await waitForConfirmation(sigBytes);
      setNotice({ kind: 'ok', text: okText, sig });
      const fresh = await fetchJar(lookup.owner, lookup.mint);
      setLookup({ ...lookup, jar: fresh });
      return true;
    } catch (e) {
      setNotice({ kind: 'err', text: errText(e) });
    } finally {
      setBusy(null);
    }
  };

  const createJar = async () => {
    if (!conn || !lookup) return;
    try {
      const { tokenProgram } = await fetchMint(lookup.mint);
      await send('create', [await createJarInstruction(conn.address, lookup.mint, tokenProgram)], 'Jar created.');
    } catch (e) {
      setNotice({ kind: 'err', text: errText(e) });
    }
  };

  const tip = async () => {
    if (!jar || !conn) return;
    const amount = parseAmount(tipText, jar.decimals);
    if (amount === null) return setNotice({ kind: 'err', text: `Enter an amount with at most ${jar.decimals} decimals.` });
    if (amount === 0n) return setNotice({ kind: 'err', text: 'A tip of 0 is not allowed.' });
    if (amount > maxTipRaw(jar.decimals)) {
      return setNotice({ kind: 'err', text: `The maximum tip is ${MAX_TIP_TOKENS.toLocaleString('en-US')} tokens.` });
    }
    const ok = await send('tip', [await tipInstruction(conn.address, jar, amount)], 'Tip sent. Thank you!');
    if (ok) setTipText('');
  };

  const withdraw = async () => {
    if (!jar || !conn) return;
    await send('withdraw', await withdrawInstructions(conn.address, jar), 'Withdrawn to your wallet.');
  };

  const isOwner = !!conn && !!lookup && conn.address === lookup.owner;
  const tipAmount = jar ? parseAmount(tipText, jar.decimals) : null;

  return (
    <div className="page">
      <header className="top">
        <div className="brand">
          <span className="logo" aria-hidden>🌰</span>
          <div>
            <h1>Acorn Jar</h1>
            <p className="tag">A community tip jar for any SPL token.</p>
          </div>
        </div>
        <div className="wallet">
          {conn ? (
            <>
              <span className="pill" title={conn.address}>{conn.wallet.name} · {short(conn.address)}</span>
              <button className="btn ghost" onClick={disconnect}>Disconnect</button>
            </>
          ) : (
            <div className="picker-wrap">
              <button className="btn" disabled={busy === 'connect'} onClick={() => setPicker((v) => !v)}>
                {busy === 'connect' ? 'Connecting…' : 'Connect wallet'}
              </button>
              {picker && (
                <div className="picker" role="menu">
                  {wallets.length === 0 && <p className="muted small">No Solana wallet detected. Install Phantom, Solflare or Backpack and reload.</p>}
                  {wallets.map((w) => (
                    <button key={w.name} className="picker-item" role="menuitem" onClick={() => connect(w)}>
                      {w.icon && <img src={w.icon} alt="" width={20} height={20} />}
                      {w.name}
                    </button>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>
      </header>

      <main>
        <section className="card">
          <h2>Find a jar</h2>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void doLookup(ownerInput, mintInput);
            }}
          >
            <label htmlFor="owner">Owner address</label>
            <div className="row">
              <input id="owner" value={ownerInput} onChange={(e) => setOwnerInput(e.target.value)} placeholder="Wallet address of the jar owner" spellCheck={false} autoComplete="off" />
              <button className="btn" type="submit" disabled={busy === 'lookup'}>{busy === 'lookup' ? 'Looking…' : 'Look up'}</button>
            </div>
            {conn && ownerInput !== conn.address && (
              <button type="button" className="link" onClick={() => setOwnerInput(conn.address)}>Use my address</button>
            )}
            <details>
              <summary>Token mint</summary>
              <input aria-label="Token mint" value={mintInput} onChange={(e) => setMintInput(e.target.value)} spellCheck={false} autoComplete="off" />
              <p className="muted small">Each jar is tied to one owner and one mint. Default: the Acorn token.</p>
            </details>
          </form>
        </section>

        {notice && (
          <div className={`notice ${notice.kind}`} role="status">
            {notice.text}{' '}
            {notice.sig && <a href={explorer(notice.sig)} target="_blank" rel="noreferrer noopener">View transaction</a>}
          </div>
        )}

        {lookup && !jar && (
          <section className="card">
            <h2>No jar yet</h2>
            <p>
              <code>{short(lookup.owner)}</code> has no jar for this token.
            </p>
            {isOwner ? (
              <button className="btn" disabled={!!busy} onClick={createJar}>{busy === 'create' ? 'Creating…' : 'Create my jar'}</button>
            ) : (
              <p className="muted small">Only the owner can create their own jar{conn ? '.' : ' - connect that wallet to create it.'}</p>
            )}
          </section>
        )}

        {jar && (
          <>
            <section className="card jar">
              <h2>Jar of <code title={jar.owner}>{short(jar.owner)}</code>{isOwner && <span className="pill small-pill">you</span>}</h2>
              <div className="stats">
                <div className="stat">
                  <span className="label">Balance</span>
                  <span className="value">{formatAmount(jar.balance, jar.decimals)}</span>
                </div>
                <div className="stat">
                  <span className="label">Tips received</span>
                  <span className="value">{jar.tipCount.toString()}</span>
                </div>
              </div>
              <dl className="meta">
                <dt>Jar</dt><dd><code>{jar.address}</code></dd>
                <dt>Mint</dt><dd><code>{jar.mint}</code></dd>
                <dt>Program</dt><dd><code>{PROGRAM_ID}</code> <span className="muted">({CLUSTER})</span></dd>
              </dl>
            </section>

            <section className="card">
              <h2>Send a tip</h2>
              {!conn ? (
                <p className="muted">Connect a wallet to tip this jar.</p>
              ) : (
                <form
                  onSubmit={(e) => {
                    e.preventDefault();
                    void tip();
                  }}
                >
                  <label htmlFor="amount">Amount (max {MAX_TIP_TOKENS.toLocaleString('en-US')} per tip)</label>
                  <div className="row">
                    <input id="amount" inputMode="decimal" value={tipText} onChange={(e) => setTipText(e.target.value)} placeholder="0.0" autoComplete="off" />
                    <button className="btn" type="submit" disabled={!!busy || !tipText || tipAmount === null || tipAmount === 0n}>
                      {busy === 'tip' ? 'Sending…' : 'Tip'}
                    </button>
                  </div>
                  {myBalance !== null && <p className="muted small">Your balance: {formatAmount(myBalance, jar.decimals)}</p>}
                </form>
              )}
            </section>

            {isOwner && (
              <section className="card">
                <h2>Owner</h2>
                <p className="muted">Withdraw the whole vault balance to your wallet.</p>
                <button className="btn" disabled={!!busy || jar.balance === 0n} onClick={withdraw}>
                  {busy === 'withdraw' ? 'Withdrawing…' : `Withdraw ${formatAmount(jar.balance, jar.decimals)}`}
                </button>
              </section>
            )}
          </>
        )}
      </main>

      <footer>
        <span>No fees. Only the owner can withdraw.</span>
        <a href="https://github.com/MyrmexAI/acorn-jar-is-a-community-02d03091" target="_blank" rel="noreferrer noopener">Source</a>
      </footer>
    </div>
  );
}

function errText(e: unknown): string {
  const m = e instanceof Error ? e.message : String(e);
  return /reject|denied|cancel/i.test(m) ? 'Request was rejected in the wallet.' : m;
}
