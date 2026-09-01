import { useState } from 'react';
import { useAuth } from '../context/AuthContext';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Mail, Lock, Eye, EyeOff, AlertCircle, ArrowRight, ArrowLeft,
  Github, Chrome, Apple, MessageSquare, Smartphone, Wand2, CheckCircle2, ShieldCheck, Zap
} from 'lucide-react';
import { API_Base } from '../lib/apiClient';

const OAUTH_PROVIDERS = [
  { id: 'google', label: 'Google', icon: Chrome },
  { id: 'github', label: 'GitHub', icon: Github },
];

const MORE_PROVIDERS = [
  { id: 'discord', label: 'Discord', icon: MessageSquare },
  { id: 'apple', label: 'Apple', icon: Apple },
];

const FEATURES = [
  { title: 'Sovereign analytics', desc: 'Every query stays in your edge region, ensuring complete data privacy.' },
  { title: 'Vector search', desc: 'Qdrant-powered HNSW similarity search over your structured data.' },
  { title: 'Visual worksheets', desc: 'Beautiful dashboards that seamlessly compile down to optimized SQL.' },
];

export default function AuthPage() {
  const { login, signup } = useAuth();

  // 'signin' | 'signup' | 'magic' | 'otp-send' | 'otp-verify'
  const [view, setView] = useState('signin');
  const [showMore, setShowMore] = useState(false);

  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [confirm, setConfirm] = useState('');
  const [showPw, setShowPw] = useState(false);
  const [remember, setRemember] = useState(true);

  const [magicEmail, setMagicEmail] = useState('');
  const [magicSent, setMagicSent] = useState(false);

  const [phone, setPhone] = useState('');
  const [otp, setOtp] = useState('');

  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);

  const oauthUrl = (provider) => `${API_Base}/v1/auth/authorize/${provider}`;

  const resetError = () => setError('');

  const submitSignIn = async (e) => {
    e.preventDefault();
    resetError();
    setLoading(true);
    const res = await login(email, password);
    setLoading(false);
    if (!res.success) setError(res.message);
  };

  const submitSignUp = async (e) => {
    e.preventDefault();
    resetError();
    if (password !== confirm) return setError('Passwords do not match.');
    if (password.length < 8) return setError('Use at least 8 characters.');
    setLoading(true);
    const res = await signup(email, password);
    setLoading(false);
    if (!res.success) setError(res.message);
  };

  const submitMagic = async (e) => {
    e.preventDefault();
    resetError();
    setLoading(true);
    try {
      const r = await fetch(`${API_Base}/v1/auth/magic-link`, {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ email: magicEmail }),
      });
      const j = await r.json();
      if (j.success) setMagicSent(true);
      else setError(j.message || 'Unable to send link.');
    } catch { setError('Network error.'); }
    setLoading(false);
  };

  const submitOtpSend = async (e) => {
    e.preventDefault();
    resetError();
    setLoading(true);
    try {
      const r = await fetch(`${API_Base}/v1/auth/phone/send`, {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ phone }),
      });
      const j = await r.json();
      if (j.success) setView('otp-verify');
      else setError(j.message || 'Unable to send code.');
    } catch { setError('Network error.'); }
    setLoading(false);
  };

  const submitOtpVerify = async (e) => {
    e.preventDefault();
    resetError();
    setLoading(true);
    try {
      const r = await fetch(`${API_Base}/v1/auth/phone/verify`, {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ phone, code: otp }),
      });
      const j = await r.json();
      if (j.success && j.data) {
        localStorage.setItem('stackhouse_access_token', j.data.access_token);
        localStorage.setItem('stackhouse_refresh_token', j.data.refresh_token);
        window.location.reload();
      } else setError(j.message || 'Invalid code.');
    } catch { setError('Network error.'); }
    setLoading(false);
  };

  const goBackToMain = () => {
    setView('signin');
    setShowMore(false);
    setMagicSent(false);
    resetError();
  };

  const title = {
    signin:       { eyebrow: 'Welcome back', heading: 'Sign in to Stackhouse' },
    signup:       { eyebrow: 'Get started',  heading: 'Create your account' },
    magic:        { eyebrow: 'Magic link',   heading: 'Passwordless sign in' },
    'otp-send':   { eyebrow: 'Phone sign-in',heading: 'Verify with your phone' },
    'otp-verify': { eyebrow: 'Phone sign-in',heading: 'Enter the 6-digit code' },
  }[view];

  return (
    <div className="flex min-h-screen w-full bg-background text-foreground font-sans selection:bg-primary/20">
      {/* ── Left Narrative Rail (Desktop) ─────────────────────────────── */}
      <aside className="dark relative hidden lg:flex lg:w-4/12 flex-col justify-between overflow-hidden bg-background p-5 text-foreground border-r border-border">
        {/* Single soft brand-tinted glow — kept deliberately restrained */}
        <div className="absolute -top-[20%] -left-[10%] h-[500px] w-[500px] rounded-full bg-primary/10 blur-[100px]" />

        <div className="relative z-10 flex items-center gap-2">
          <div className="flex h-10 w-10 shrink-0 items-center justify-center overflow-hidden rounded-lg border border-border">
            <img src="/logo.svg" alt="Stackhouse" className="h-full w-full" />
          </div>
          <div>
            <div className="font-serif text-lg font-bold tracking-tight text-foreground">Stackhouse</div>
            <div className="text-[10px] font-bold uppercase tracking-[0.25em] text-muted-foreground">Open Source Backend</div>
          </div>
        </div>

        <div className="relative z-10 max-w-md my-auto">
          <div className="inline-flex items-center gap-1 rounded-full border border-border bg-card px-2 py-0.5 mb-4">
            <Zap className="h-2.5 w-2.5 text-primary" />
            <span className="text-xs font-semibold tracking-wide text-muted-foreground">Self-hosted and MIT licensed</span>
          </div>
          <h1 className="font-serif text-[44px] leading-[1.1] font-medium tracking-tight mb-3 text-foreground">
            The database that thinks with you.
          </h1>
          <p className="text-[15px] leading-relaxed text-muted-foreground mb-5">
            Experience sovereign analytics, vector search, and visual worksheets—all within a calm, powerful surface.
          </p>

          <div className="space-y-3">
            {FEATURES.map((f) => (
              <div
                key={f.title}
                className="flex items-start gap-3 rounded-lg border border-border bg-card p-3"
              >
                <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-muted text-muted-foreground">
                  <CheckCircle2 className="h-4 w-4" />
                </div>
                <div>
                  <p className="text-sm font-semibold text-foreground">{f.title}</p>
                  <p className="mt-0.5 text-[13px] text-muted-foreground leading-relaxed">{f.desc}</p>
                </div>
              </div>
            ))}
          </div>
        </div>

        <div className="relative z-10 flex items-center gap-1 text-xs font-medium text-muted-foreground">
          <ShieldCheck className="h-3 w-3" />
          <span>Self-hosted · Open source · MIT licensed</span>
        </div>
      </aside>

      {/* ── Right Form Panel ───────────────────────────────────────────── */}
      <main className="flex flex-1 flex-col items-center justify-center relative overflow-hidden bg-background">
        {/* Subtle background glow for right panel */}
        <div className="absolute inset-0 bg-[radial-gradient(ellipse_at_top_right,_var(--tw-gradient-stops))] from-primary/5 via-background to-background pointer-events-none" />

        <div className="relative w-full max-w-[440px] px-3 py-5 animate-in fade-in slide-in-from-bottom-8 duration-700 fill-mode-both">
          
          {/* Mobile brand header */}
          <div className="mb-5 flex items-center justify-center gap-2 lg:hidden">
            <div className="flex h-10 w-10 shrink-0 items-center justify-center overflow-hidden rounded-lg">
              <img src="/logo.svg" alt="Stackhouse" className="h-full w-full" />
            </div>
            <span className="font-serif text-lg font-semibold">Stackhouse</span>
          </div>

          {/* Card Container for Form */}
          <div className="rounded-[24px] border border-border/50 bg-card/60 p-4 sm:p-5 shadow-none backdrop-blur-xl relative overflow-hidden">
            
            {/* Decorative top gradient line on card */}
            <div className="absolute top-0 left-0 w-full h-0.5 bg-gradient-to-r from-transparent via-primary/50 to-transparent opacity-50" />

            {view !== 'signin' && view !== 'signup' && (
              <button
                onClick={goBackToMain}
                className="mb-3 inline-flex items-center gap-0.5.5 text-sm font-medium text-muted-foreground hover:text-foreground transition-colors group"
              >
                <ArrowLeft className="h-3 w-3 group-hover:-translate-x-1 transition-transform" /> Back
              </button>
            )}

            <div className="mb-4">
              <p className="text-xs font-bold uppercase tracking-[0.2em] text-primary mb-1">{title.eyebrow}</p>
              <h2 className="font-serif text-xl font-semibold leading-tight tracking-tight text-foreground">{title.heading}</h2>
            </div>

            {error && (
              <div className="mb-3 flex items-start gap-2 rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2 text-sm text-destructive animate-in fade-in slide-in-from-top-2">
                <AlertCircle className="mt-0.5 h-3 w-3 shrink-0" />
                <span className="font-medium">{error}</span>
              </div>
            )}

            {/* ── Sign In View ─────────────────────────────────────────── */}
            {view === 'signin' && (
              <div className="animate-in fade-in duration-500">
                <div className="mb-3 grid grid-cols-2 gap-2">
                  {OAUTH_PROVIDERS.map((p) => {
                    const Icon = p.icon;
                    return (
                      <a key={p.id} href={oauthUrl(p.id)}
                         className="inline-flex h-10 items-center justify-center gap-1 rounded-md border border-border bg-background/50 text-sm font-semibold transition-all hover:bg-muted hover:border-foreground/20 hover:shadow-none hover:-translate-y-0.5">
                        <Icon className="h-3 w-3" /> {p.label}
                      </a>
                    );
                  })}
                </div>

                {showMore && (
                  <div className="mb-3 grid grid-cols-2 gap-2 animate-in fade-in slide-in-from-top-2">
                    {MORE_PROVIDERS.map((p) => {
                      const Icon = p.icon;
                      return (
                        <a key={p.id} href={oauthUrl(p.id)}
                           className="inline-flex h-10 items-center justify-center gap-1 rounded-md border border-border bg-background/50 text-sm font-semibold transition-all hover:bg-muted hover:border-foreground/20 hover:shadow-none hover:-translate-y-0.5">
                          <Icon className="h-3 w-3" /> {p.label}
                        </a>
                      );
                    })}
                    <AltButton icon={Wand2} label="Magic link" onClick={() => setView('magic')} />
                    <AltButton icon={Smartphone} label="Phone OTP" onClick={() => setView('otp-send')} />
                  </div>
                )}

                {!showMore && (
                  <div className="mb-3 text-center">
                    <button onClick={() => setShowMore(true)}
                      className="text-xs font-medium text-muted-foreground hover:text-foreground transition-colors hover:underline">
                      Show more sign-in options
                    </button>
                  </div>
                )}

                <div className="relative my-3">
                  <div className="absolute inset-0 flex items-center"><span className="w-full border-t border-border/60" /></div>
                  <div className="relative flex justify-center"><span className="bg-card px-3 text-xs font-medium uppercase tracking-widest text-muted-foreground">or</span></div>
                </div>

                <form onSubmit={submitSignIn} className="space-y-3">
                  <FieldEmail value={email} onChange={setEmail} />
                  <FieldPassword value={password} onChange={setPassword} show={showPw} toggle={() => setShowPw(!showPw)} />
                  
                  <div className="flex items-center justify-between pt-0.5">
                    <label className="inline-flex items-center gap-1 text-sm font-medium text-muted-foreground cursor-pointer select-none group">
                      <div className="relative flex items-center justify-center h-3 w-3">
                        <input type="checkbox" checked={remember} onChange={(e) => setRemember(e.target.checked)}
                          className="peer appearance-none h-3 w-3 rounded-[4px] border border-border/80 bg-background checked:bg-primary checked:border-primary transition-all cursor-pointer" />
                        <CheckCircle2 className="absolute h-2 w-2 text-primary-foreground opacity-0 peer-checked:opacity-100 transition-opacity pointer-events-none" />
                      </div>
                      <span className="group-hover:text-foreground transition-colors">Remember me</span>
                    </label>
                    <button type="button"
                      onClick={() => { setMagicEmail(email); setView('magic'); resetError(); }}
                      className="text-sm font-medium text-primary hover:text-primary/80 transition-colors hover:underline">
                      Forgot password?
                    </button>
                  </div>
                  
                  <Button type="submit" className="w-full h-10 rounded-md text-[15px] font-semibold shadow-none shadow-none hover:shadow-none hover:shadow-none-primary/30 hover:-translate-y-0.5 transition-all duration-300" disabled={loading}>
                    {loading ? 'Signing in…' : <span className="flex items-center">Sign in <ArrowRight className="ml-1 h-3 w-3" /></span>}
                  </Button>
                </form>

                <p className="mt-4 text-center text-sm text-muted-foreground">
                  Don't have an account?{' '}
                  <button onClick={() => { setView('signup'); resetError(); }} className="font-semibold text-primary hover:text-primary/80 hover:underline transition-colors">
                    Register now
                  </button>
                </p>
              </div>
            )}

            {/* ── Sign Up View ─────────────────────────────────────────── */}
            {view === 'signup' && (
              <div className="animate-in fade-in slide-in-from-right-4 duration-500">
                <form onSubmit={submitSignUp} className="space-y-3">
                  <FieldEmail value={email} onChange={setEmail} label="Work email" />
                  <FieldPassword value={password} onChange={setPassword} show={showPw} toggle={() => setShowPw(!showPw)} label="Password" hint="At least 8 characters required." />
                  <div className="space-y-1">
                    <Label className="text-sm font-medium text-foreground">Confirm password</Label>
                    <div className="relative group">
                      <Lock className="absolute left-3.5 top-1/2 h-3 w-3 -translate-y-1/2 text-muted-foreground group-focus-within:text-primary transition-colors" />
                      <Input 
                        type="password" 
                        value={confirm} 
                        onChange={(e) => setConfirm(e.target.value)} 
                        required 
                        className="h-10 pl-5 rounded-md bg-background/50 border-border/60 focus-visible:ring-primary/30 transition-all hover:border-border" 
                        placeholder="••••••••"
                      />
                    </div>
                  </div>
                  <Button type="submit" className="w-full h-10 rounded-md text-[15px] font-semibold shadow-none shadow-none hover:shadow-none hover:shadow-none-primary/30 hover:-translate-y-0.5 transition-all duration-300 mt-1" disabled={loading}>
                    {loading ? 'Creating account…' : <span className="flex items-center">Create account <ArrowRight className="ml-1 h-3 w-3" /></span>}
                  </Button>
                </form>

                <p className="mt-3 text-center text-xs text-muted-foreground leading-relaxed px-3">
                  By signing up, you agree to our <a href="#" className="underline hover:text-foreground">Terms of Service</a> and <a href="#" className="underline hover:text-foreground">Privacy Policy</a>.
                </p>

                <div className="relative my-3">
                  <div className="absolute inset-0 flex items-center"><span className="w-full border-t border-border/60" /></div>
                </div>

                <p className="text-center text-sm text-muted-foreground">
                  Already have an account?{' '}
                  <button onClick={() => { setView('signin'); resetError(); }} className="font-semibold text-primary hover:text-primary/80 hover:underline transition-colors">
                    Sign in
                  </button>
                </p>
              </div>
            )}

            {/* ── Magic Link View ──────────────────────────────────────── */}
            {view === 'magic' && (
              <div className="animate-in fade-in slide-in-from-right-4 duration-500">
                {magicSent ? (
                  <div className="rounded-lg border border-primary/20 bg-primary/5 p-4 text-center shadow-none">
                    <div className="mx-auto mb-3 flex h-12 w-12 items-center justify-center rounded-full bg-primary/10 ring-4 ring-primary/5">
                      <Mail className="h-5 w-5 text-primary" />
                    </div>
                    <h3 className="font-serif text-base font-semibold mb-1 text-foreground">Check your inbox</h3>
                    <p className="text-sm text-muted-foreground leading-relaxed">
                      We've sent a secure sign-in link to <br/><span className="font-medium text-foreground">{magicEmail}</span>.
                    </p>
                    <div className="mt-3 pt-3 border-t border-primary/10">
                      <p className="text-xs text-muted-foreground">
                        Didn't receive the email?{' '}
                        <button onClick={() => setMagicSent(false)} className="font-medium text-primary hover:underline">Click to resend</button>
                      </p>
                    </div>
                  </div>
                ) : (
                  <form onSubmit={submitMagic} className="space-y-3">
                    <p className="text-sm text-muted-foreground leading-relaxed mb-1">
                      Enter your email address and we'll send you a magic link to sign in instantly. No password required.
                    </p>
                    <FieldEmail value={magicEmail} onChange={setMagicEmail} />
                    <Button type="submit" className="w-full h-10 rounded-md text-[15px] font-semibold shadow-none shadow-none hover:shadow-none hover:-translate-y-0.5 transition-all duration-300" disabled={loading}>
                      {loading ? 'Sending link…' : <span className="flex items-center">Send magic link <ArrowRight className="ml-1 h-3 w-3" /></span>}
                    </Button>
                  </form>
                )}
              </div>
            )}

            {/* ── Phone OTP Send View ─────────────────────────────────── */}
            {view === 'otp-send' && (
              <div className="animate-in fade-in slide-in-from-right-4 duration-500">
                <form onSubmit={submitOtpSend} className="space-y-3">
                  <p className="text-sm text-muted-foreground leading-relaxed mb-1">
                    Enter your phone number. We'll text you a secure 6-digit code to verify your identity.
                  </p>
                  <div className="space-y-1">
                    <Label className="text-sm font-medium text-foreground">Phone number</Label>
                    <div className="relative group">
                      <Smartphone className="absolute left-3.5 top-1/2 h-3 w-3 -translate-y-1/2 text-muted-foreground group-focus-within:text-primary transition-colors" />
                      <Input 
                        type="tel" 
                        placeholder="+1 (555) 000-0000" 
                        value={phone} 
                        onChange={(e) => setPhone(e.target.value)} 
                        required 
                        className="h-10 pl-5 rounded-md bg-background/50 border-border/60 focus-visible:ring-primary/30 transition-all hover:border-border" 
                      />
                    </div>
                  </div>
                  <Button type="submit" className="w-full h-10 rounded-md text-[15px] font-semibold shadow-none shadow-none hover:shadow-none hover:-translate-y-0.5 transition-all duration-300" disabled={loading}>
                    {loading ? 'Sending code…' : <span className="flex items-center">Send code <ArrowRight className="ml-1 h-3 w-3" /></span>}
                  </Button>
                </form>
              </div>
            )}

            {/* ── Phone OTP Verify View ───────────────────────────────── */}
            {view === 'otp-verify' && (
              <div className="animate-in fade-in slide-in-from-right-4 duration-500">
                <form onSubmit={submitOtpVerify} className="space-y-3">
                  <p className="text-sm text-muted-foreground leading-relaxed">
                    We sent a verification code to <span className="font-semibold text-foreground">{phone}</span>.
                  </p>
                  <div className="space-y-2">
                    <Label className="text-sm font-medium text-foreground">6-digit security code</Label>
                    <Input
                      type="text"
                      inputMode="numeric"
                      maxLength={6}
                      placeholder="• • • • • •"
                      value={otp}
                      onChange={(e) => setOtp(e.target.value.replace(/\D/g, ''))}
                      required
                      className="h-12 text-center text-lg font-mono tracking-[0.5em] rounded-md bg-background/50 border-border/60 focus-visible:ring-primary/30 transition-all hover:border-border shadow-none"
                    />
                  </div>
                  <Button type="submit" className="w-full h-10 rounded-md text-[15px] font-semibold shadow-none shadow-none hover:shadow-none hover:-translate-y-0.5 transition-all duration-300" disabled={loading || otp.length < 6}>
                    {loading ? 'Verifying identity…' : 'Verify & Sign In'}
                  </Button>
                  <div className="text-center">
                    <button type="button" onClick={() => setView('otp-send')}
                      className="text-sm font-medium text-muted-foreground hover:text-primary transition-colors hover:underline">
                      Wrong number? Try again
                    </button>
                  </div>
                </form>
              </div>
            )}

          </div>
        </div>
      </main>
    </div>
  );
}

/* ───────── Beautiful Subcomponents ───────── */

function FieldEmail({ value, onChange, label = 'Email address' }) {
  return (
    <div className="space-y-1">
      <Label className="text-sm font-medium text-foreground">{label}</Label>
      <div className="relative group">
        <Mail className="absolute left-3.5 top-1/2 h-3 w-3 -translate-y-1/2 text-muted-foreground group-focus-within:text-primary transition-colors" />
        <Input
          type="email"
          placeholder="name@company.com"
          value={value}
          onChange={(e) => onChange(e.target.value)}
          autoComplete="email"
          required
          className="h-10 pl-5 rounded-md bg-background/50 border-border/60 focus-visible:ring-primary/30 transition-all hover:border-border"
        />
      </div>
    </div>
  );
}

function FieldPassword({ value, onChange, show, toggle, label = 'Password', hint }) {
  return (
    <div className="space-y-1">
      <Label className="text-sm font-medium text-foreground">{label}</Label>
      <div className="relative group">
        <Lock className="absolute left-3.5 top-1/2 h-3 w-3 -translate-y-1/2 text-muted-foreground group-focus-within:text-primary transition-colors" />
        <Input
          type={show ? 'text' : 'password'}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          autoComplete="current-password"
          required
          placeholder="••••••••"
          className="h-10 pl-5 pr-5 rounded-md bg-background/50 border-border/60 focus-visible:ring-primary/30 transition-all hover:border-border"
        />
        <button type="button" onClick={toggle}
          className="absolute right-3 top-1/2 -translate-y-1/2 p-0.5.5 text-muted-foreground hover:text-foreground hover:bg-muted rounded-md transition-all">
          {show ? <EyeOff className="h-3 w-3" /> : <Eye className="h-3 w-3" />}
        </button>
      </div>
      {hint && <p className="text-xs text-muted-foreground mt-0.5.5 ml-0.5">{hint}</p>}
    </div>
  );
}

function AltButton({ icon: Icon, label, onClick }) {
  return (
    <button onClick={onClick}
      className="inline-flex h-10 items-center justify-center gap-1 rounded-md border border-border bg-background/50 text-sm font-semibold transition-all hover:bg-muted hover:border-foreground/20 hover:shadow-none hover:-translate-y-0.5">
      <Icon className="h-3 w-3" /> {label}
    </button>
  );
}
