import { useEffect, useState, type FormEvent } from "react";
import { Button } from "../components/ui";
import { SectionIcon } from "../components/SectionIcon";
import {
  providerPurposes,
  type ProviderProfile,
  type ProviderProfileId,
  type ProviderPurpose,
} from "../models/providerProfile";
import {
  createProviderProfile,
  deleteProviderProfile,
  listProviderProfiles,
  updateProviderProfile,
} from "../services/providerProfileService";
import { getIpcError } from "../services/projectService";

type ProfileEditor = { purpose: ProviderPurpose; profileId: ProviderProfileId | null };

type ProfileDraft = {
  profileName: string;
  provider: string;
  apiKey: string;
  baseUrl: string;
  model: string;
  isDefault: boolean;
  voice: string;
};

const purposeCopy: Record<ProviderPurpose, { label: string; description: string }> = {
  TEXT: { label: "Text", description: "Script writing and structured scene generation." },
  IMAGE: { label: "Image", description: "Still-image and storyboard artwork providers." },
  VIDEO: { label: "Video", description: "Image-to-video and text-to-video providers." },
  TTS: { label: "TTS", description: "Voice and narration synthesis providers." },
};

const royalTechnoModels = ["omni-flash", "veo-3.1", "seedance-2.0"] as const;

function isRoyalTechnoModel(model: string) {
  return royalTechnoModels.some((option) => option === model);
}

const emptyDraft = (isDefault = false): ProfileDraft => ({
  profileName: "",
  provider: "",
  apiKey: "",
  baseUrl: "",
  model: "",
  isDefault,
  voice: "",
});

const emptyGroups = (): Record<ProviderPurpose, ProviderProfile[]> => ({
  TEXT: [],
  IMAGE: [],
  VIDEO: [],
  TTS: [],
});

export function SettingsPage() {
  const [profiles, setProfiles] = useState<Record<ProviderPurpose, ProviderProfile[]>>(
    emptyGroups,
  );
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [editor, setEditor] = useState<ProfileEditor | null>(null);
  const [draft, setDraft] = useState<ProfileDraft>(emptyDraft());
  const [showApiKey, setShowApiKey] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  const groupByPurpose = (list: ProviderProfile[]) => {
    const groups = emptyGroups();
    for (const profile of list) groups[profile.purpose].push(profile);
    return groups;
  };

  const reload = () => {
    setLoading(true);
    setLoadError(null);
    listProviderProfiles()
      .then((list) => setProfiles(groupByPurpose(list)))
      .catch((error) => setLoadError(getIpcError(error).message))
      .finally(() => setLoading(false));
  };

  useEffect(() => {
    reload();
  }, []);

  const openNewProfile = (purpose: ProviderPurpose) => {
    setEditor({ purpose, profileId: null });
    setDraft(emptyDraft(profiles[purpose].length === 0));
    setShowApiKey(false);
    setFormError(null);
  };

  const openEditProfile = (profile: ProviderProfile) => {
    setEditor({ purpose: profile.purpose, profileId: profile.id });
    setDraft({
      profileName: profile.profile_name,
      provider: profile.provider,
      apiKey: "",
      baseUrl: profile.base_url ?? "",
      model: profile.model,
      isDefault: profile.is_default,
      voice: profile.voice ?? "",
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

  const updateDraft = <Field extends keyof ProfileDraft>(field: Field, value: ProfileDraft[Field]) => {
    setDraft((current) => ({ ...current, [field]: value }));
    setFormError(null);
  };

  const updateVideoProvider = (value: string) => {
    setDraft((current) => ({
      ...current,
      provider: value,
      model: value === "royaltechno" && !current.model.trim() ? "omni-flash" : current.model,
    }));
    setFormError(null);
  };

  const handleApplyDraft = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!editor) return;

    const profileName = draft.profileName.trim();
    const provider = draft.provider.trim();
    if (!profileName || !provider) {
      setFormError("Profile Name and Provider are required.");
      return;
    }
    if (!editor.profileId && !draft.apiKey.trim()) {
      setFormError("API Key is required.");
      return;
    }

    const baseUrl = draft.baseUrl.trim() || null;
    const model = draft.model.trim();
    const voice = draft.voice.trim() || null;

    setSaving(true);
    setFormError(null);
    try {
      if (editor.profileId) {
        await updateProviderProfile(editor.profileId, {
          profile_name: profileName,
          provider,
          base_url: baseUrl,
          model,
          voice,
          is_default: draft.isDefault,
          api_key: draft.apiKey.trim() ? draft.apiKey : null,
        });
      } else {
        await createProviderProfile({
          purpose: editor.purpose,
          profile_name: profileName,
          provider,
          base_url: baseUrl,
          model,
          voice,
          is_default: draft.isDefault,
          api_key: draft.apiKey,
        });
      }
      closeEditor();
      reload();
    } catch (error) {
      setFormError(getIpcError(error).message);
    } finally {
      setSaving(false);
    }
  };

  const setDefaultProfile = async (profile: ProviderProfile) => {
    try {
      await updateProviderProfile(profile.id, {
        profile_name: profile.profile_name,
        provider: profile.provider,
        base_url: profile.base_url,
        model: profile.model,
        voice: profile.voice,
        is_default: true,
        api_key: null,
      });
      reload();
    } catch (error) {
      setLoadError(getIpcError(error).message);
    }
  };

  const removeProfile = async (profile: ProviderProfile) => {
    if (!window.confirm(`Remove the "${profile.profile_name}" profile?`)) return;
    try {
      await deleteProviderProfile(profile.id);
      if (editor?.profileId === profile.id) closeEditor();
      reload();
    } catch (error) {
      setLoadError(getIpcError(error).message);
    }
  };

  return (
    <div className="workspace-page settings-page">
      <header className="workspace-header">
        <div className="workspace-heading">
          <span className="workspace-heading-icon" aria-hidden="true"><SectionIcon name="settings" /></span>
          <div><p className="eyebrow">Application preferences</p><h1>Settings</h1><p className="lead">Organize provider profiles for every AI purpose.</p></div>
        </div>
      </header>

      {loadError ? (
        <div className="settings-notice" role="alert">
          <strong>Failed to load provider profiles.</strong>
          <p>{loadError}</p>
        </div>
      ) : null}

      <div className="provider-purpose-list">
        {providerPurposes.map((purpose) => {
          const purposeProfiles = loading ? [] : profiles[purpose];
          const copy = purposeCopy[purpose];
          return (
            <section className="provider-purpose-panel" aria-labelledby={`${purpose}-providers-title`} key={purpose}>
              <header className="provider-purpose-header">
                <div><p className="eyebrow">{purpose.toLowerCase()} providers</p><h2 id={`${purpose}-providers-title`}>{copy.label}</h2><p>{copy.description}</p></div>
                <Button variant="secondary" onClick={() => openNewProfile(purpose)}>Add profile</Button>
              </header>

              {loading ? (
                <div className="provider-empty"><p>Loading...</p></div>
              ) : purposeProfiles.length === 0 ? (
                <div className="provider-empty"><p>No {copy.label.toLowerCase()} provider profiles configured.</p></div>
              ) : (
                <ul className="provider-profile-list" aria-label={`${copy.label} provider profiles`}>
                  {purposeProfiles.map((profile) => (
                    <li className={profile.is_default ? "provider-profile-card default" : "provider-profile-card"} key={profile.id}>
                      <div className="provider-profile-heading"><div><h3>{profile.profile_name}</h3><p>{profile.provider}</p></div>{profile.is_default ? <span className="default-badge">Default</span> : null}</div>
                      <dl className="provider-profile-meta">
                        <div><dt>Model</dt><dd>{profile.model || "Not set"}</dd></div>
                        <div><dt>Base URL</dt><dd>{profile.base_url || "Not set"}</dd></div>
                        <div><dt>API Key</dt><dd>{profile.has_api_key ? "Configured" : "Not set"}</dd></div>
                      </dl>
                      <div className="provider-profile-actions">
                        {!profile.is_default ? <Button variant="secondary" onClick={() => setDefaultProfile(profile)}>Set Default</Button> : null}
                        <Button variant="secondary" onClick={() => openEditProfile(profile)}>Edit</Button>
                        <Button variant="secondary" onClick={() => removeProfile(profile)}>Remove</Button>
                      </div>
                    </li>
                  ))}
                </ul>
              )}

              {editor?.purpose === purpose ? (
                <form className="provider-editor" onSubmit={handleApplyDraft} aria-labelledby={`${purpose}-editor-title`}>
                  <div className="provider-editor-heading"><div><p className="eyebrow">{editor.profileId ? "Edit profile" : "New profile"}</p><h3 id={`${purpose}-editor-title`}>{editor.profileId ? "Edit profile" : "New profile"}</h3></div></div>
                  <div className="provider-editor-grid">
                    <label>Profile Name<input value={draft.profileName} onChange={(event) => updateDraft("profileName", event.currentTarget.value)} required /></label>
                    {purpose === "VIDEO" ? <label>Provider<input list="video-provider-options" value={draft.provider} onChange={(event) => updateVideoProvider(event.currentTarget.value)} placeholder="Select or enter provider" required /><datalist id="video-provider-options"><option value="openai">OpenAI-compatible</option><option value="royaltechno">RoyalTechno</option></datalist></label> : <label>Provider<input value={draft.provider} onChange={(event) => updateDraft("provider", event.currentTarget.value)} placeholder="Provider name" required /></label>}
                    <label className="provider-editor-wide">API Key<span className="password-field"><input type={showApiKey ? "text" : "password"} value={draft.apiKey} onChange={(event) => updateDraft("apiKey", event.currentTarget.value)} placeholder={editor.profileId ? "Leave blank to keep current key" : "Required"} autoComplete="off" spellCheck={false} /><Button variant="secondary" onClick={() => setShowApiKey((visible) => !visible)} aria-pressed={showApiKey}>{showApiKey ? "Hide" : "Show"}</Button></span></label>
                    <label>Base URL<input type="url" value={draft.baseUrl} onChange={(event) => updateDraft("baseUrl", event.currentTarget.value)} placeholder="https://api.example.com/v1" /></label>
                    {purpose === "VIDEO" && draft.provider === "royaltechno" ? <label>Model<select value={isRoyalTechnoModel(draft.model) ? draft.model : "custom"} onChange={(event) => { const value = event.currentTarget.value; if (value !== "custom") updateDraft("model", value); }}><option value="omni-flash">omni-flash</option><option value="veo-3.1">veo-3.1</option><option value="seedance-2.0">seedance-2.0</option><option value="custom">Custom model</option></select>{!isRoyalTechnoModel(draft.model) ? <input value={draft.model} onChange={(event) => updateDraft("model", event.currentTarget.value)} placeholder="Custom model name" /> : null}</label> : <label>Model<input value={draft.model} onChange={(event) => updateDraft("model", event.currentTarget.value)} placeholder="Provider model name" /></label>}
                    {purpose === "TTS" ? <label className="provider-editor-wide">Voice<input value={draft.voice} onChange={(event) => updateDraft("voice", event.currentTarget.value)} placeholder="Voice name" /></label> : null}
                    <label className="default-checkbox provider-editor-wide"><input type="checkbox" checked={draft.isDefault} onChange={(event) => updateDraft("isDefault", event.currentTarget.checked)} /><span>Use as Default for {copy.label}</span></label>
                  </div>
                  {formError ? <p className="settings-state-error" role="alert">{formError}</p> : null}
                  <div className="settings-actions"><Button type="submit" disabled={saving}>{saving ? "Saving..." : "Save profile"}</Button><Button variant="secondary" onClick={closeEditor}>Cancel</Button></div>
                </form>
              ) : null}
            </section>
          );
        })}
      </div>
    </div>
  );
}
