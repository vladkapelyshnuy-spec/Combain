import { useRef, useState, type FormEvent } from "react";
import { Button } from "../components/ui";
import { SectionIcon } from "../components/SectionIcon";
import {
  emptyProviderProfiles,
  providerPurposes,
  type ProviderProfile,
  type ProviderProfileDraft,
  type ProviderProfileId,
  type ProviderProfilesByPurpose,
  type ProviderPurpose,
} from "../models/providerProfile";

type ProfileEditor = { purpose: ProviderPurpose; profileId: ProviderProfileId | null };

const purposeCopy: Record<ProviderPurpose, { label: string; description: string }> = {
  text: { label: "Text", description: "Script writing and structured scene generation." },
  image: { label: "Image", description: "Still-image and storyboard artwork providers." },
  video: { label: "Video", description: "Image-to-video and text-to-video providers." },
  tts: { label: "TTS", description: "Voice and narration synthesis providers." },
};

const emptyDraft = (isDefault = false): ProviderProfileDraft => ({
  profileName: "", provider: "", apiKey: "", baseUrl: "", model: "", isDefault, voice: undefined,
});

export function SettingsPage() {
  const [profiles, setProfiles] = useState<ProviderProfilesByPurpose>(emptyProviderProfiles);
  const [editor, setEditor] = useState<ProfileEditor | null>(null);
  const [draft, setDraft] = useState<ProviderProfileDraft>(emptyDraft());
  const [showApiKey, setShowApiKey] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const nextId = useRef(1);

  const openNewProfile = (purpose: ProviderPurpose) => {
    setEditor({ purpose, profileId: null });
    setDraft(emptyDraft(profiles[purpose].length === 0));
    setShowApiKey(false);
    setFormError(null);
  };

  const openEditProfile = (profile: ProviderProfile) => {
    setEditor({ purpose: profile.purpose, profileId: profile.id });
    setDraft({
      profileName: profile.profileName,
      provider: profile.provider,
      apiKey: profile.apiKey,
      baseUrl: profile.baseUrl,
      model: profile.model,
      isDefault: profile.isDefault,
      voice: profile.voice,
    });
    setShowApiKey(false);
    setFormError(null);
  };

  const closeEditor = () => {
    setEditor(null);
    setDraft(emptyDraft());
    setShowApiKey(false);
    setFormError(null);
  };

  const updateDraft = <Field extends keyof ProviderProfileDraft>(field: Field, value: ProviderProfileDraft[Field]) => {
    setDraft((current) => ({ ...current, [field]: value }));
    setFormError(null);
  };

  const handleApplyDraft = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!editor) return;
    const profileName = draft.profileName.trim();
    const provider = draft.provider.trim();
    if (!profileName || !provider) {
      setFormError("Profile Name and Provider are required.");
      return;
    }
    setProfiles((current) => {
      const existing = current[editor.purpose];
      const fields = { ...draft, profileName, provider, baseUrl: draft.baseUrl.trim(), model: draft.model.trim() };
      const prepared = draft.isDefault ? existing.map((profile) => ({ ...profile, isDefault: false })) : existing;
      const next = editor.profileId
        ? prepared.map((profile) => profile.id === editor.profileId ? { ...profile, ...fields } : profile)
        : [...prepared, { id: `profile-${nextId.current++}`, purpose: editor.purpose, ...fields }];
      return { ...current, [editor.purpose]: next };
    });
    closeEditor();
  };

  const setDefaultProfile = (purpose: ProviderPurpose, profileId: ProviderProfileId) => {
    setProfiles((current) => ({
      ...current,
      [purpose]: current[purpose].map((profile) => ({ ...profile, isDefault: profile.id === profileId })),
    }));
  };

  const removeProfile = (profile: ProviderProfile) => {
    if (!window.confirm(`Remove the “${profile.profileName}” draft profile?`)) return;
    setProfiles((current) => ({
      ...current,
      [profile.purpose]: current[profile.purpose].filter((item) => item.id !== profile.id),
    }));
    if (editor?.profileId === profile.id) closeEditor();
  };

  return (
    <div className="workspace-page settings-page">
      <header className="workspace-header">
        <div className="workspace-heading">
          <span className="workspace-heading-icon" aria-hidden="true"><SectionIcon name="settings" /></span>
          <div><p className="eyebrow">Application preferences</p><h1>Settings</h1><p className="lead">Organize provider profiles for every AI purpose.</p></div>
        </div>
        <span className="phase-badge">Frontend preview</span>
      </header>

      <div className="settings-notice" role="note">
        <strong>Backend persistence is not connected.</strong>
        <p>Profiles are temporary drafts held only in this page's React state. API keys are never saved, sent, or placed in browser storage, and leaving Settings clears them.</p>
      </div>

      <div className="provider-purpose-list">
        {providerPurposes.map((purpose) => {
          const purposeProfiles = profiles[purpose];
          const copy = purposeCopy[purpose];
          return (
            <section className="provider-purpose-panel" aria-labelledby={`${purpose}-providers-title`} key={purpose}>
              <header className="provider-purpose-header">
                <div><p className="eyebrow">{purpose} providers</p><h2 id={`${purpose}-providers-title`}>{copy.label}</h2><p>{copy.description}</p></div>
                <Button variant="secondary" onClick={() => openNewProfile(purpose)}>Add profile</Button>
              </header>

              {purposeProfiles.length === 0 ? (
                <div className="provider-empty"><p>No {copy.label.toLowerCase()} provider profiles configured.</p></div>
              ) : (
                <ul className="provider-profile-list" aria-label={`${copy.label} provider profiles`}>
                  {purposeProfiles.map((profile) => (
                    <li className={profile.isDefault ? "provider-profile-card default" : "provider-profile-card"} key={profile.id}>
                      <div className="provider-profile-heading"><div><h3>{profile.profileName}</h3><p>{profile.provider}</p></div>{profile.isDefault ? <span className="default-badge">Default</span> : null}</div>
                      <dl className="provider-profile-meta">
                        <div><dt>Model</dt><dd>{profile.model || "Not set"}</dd></div>
                        <div><dt>Base URL</dt><dd>{profile.baseUrl || "Not set"}</dd></div>
                        <div><dt>API Key</dt><dd>{profile.apiKey ? "Configured in draft" : "Not set"}</dd></div>
                      </dl>
                      <div className="provider-profile-actions">
                        {!profile.isDefault ? <Button variant="secondary" onClick={() => setDefaultProfile(purpose, profile.id)}>Set Default</Button> : null}
                        <Button variant="secondary" onClick={() => openEditProfile(profile)}>Edit</Button>
                        <Button variant="secondary" onClick={() => removeProfile(profile)}>Remove</Button>
                      </div>
                    </li>
                  ))}
                </ul>
              )}

              {editor?.purpose === purpose ? (
                <form className="provider-editor" onSubmit={handleApplyDraft} aria-labelledby={`${purpose}-editor-title`}>
                  <div className="provider-editor-heading"><div><p className="eyebrow">Unsaved local draft</p><h3 id={`${purpose}-editor-title`}>{editor.profileId ? "Edit profile" : "New profile"}</h3></div><span className="settings-connection-status">Not persisted</span></div>
                  <div className="provider-editor-grid">
                    <label>Profile Name<input value={draft.profileName} onChange={(event) => updateDraft("profileName", event.currentTarget.value)} required /></label>
                    <label>Provider<input value={draft.provider} onChange={(event) => updateDraft("provider", event.currentTarget.value)} placeholder="Provider name" required /></label>
                    <label className="provider-editor-wide">API Key<span className="password-field"><input type={showApiKey ? "text" : "password"} value={draft.apiKey} onChange={(event) => updateDraft("apiKey", event.currentTarget.value)} placeholder="Held in memory only" autoComplete="off" spellCheck={false} /><Button variant="secondary" onClick={() => setShowApiKey((visible) => !visible)} aria-pressed={showApiKey}>{showApiKey ? "Hide" : "Show"}</Button></span></label>
                    <label>Base URL<input type="url" value={draft.baseUrl} onChange={(event) => updateDraft("baseUrl", event.currentTarget.value)} placeholder="https://api.example.com/v1" /></label>
                    <label>Model<input value={draft.model} onChange={(event) => updateDraft("model", event.currentTarget.value)} placeholder="Provider model name" /></label>
                    {purpose === "tts" ? <p className="provider-editor-wide settings-field-help">Voice-specific configuration is reserved in the typed profile model for future TTS integration.</p> : null}
                    <label className="default-checkbox provider-editor-wide"><input type="checkbox" checked={draft.isDefault} onChange={(event) => updateDraft("isDefault", event.currentTarget.checked)} /><span>Use as Default for {copy.label}</span></label>
                  </div>
                  {formError ? <p className="settings-state-error" role="alert">{formError}</p> : null}
                  <div className="settings-actions"><Button type="submit">Save profile draft</Button><Button variant="secondary" onClick={closeEditor}>Cancel</Button></div>
                </form>
              ) : null}
            </section>
          );
        })}
      </div>
    </div>
  );
}
