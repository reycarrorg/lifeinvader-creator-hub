'use strict';
(() => {
  const visibility = {
    PUBLIC_TO_EVERYONE: 'Everyone',
    MUTUAL_FOLLOW_FRIENDS: 'Friends',
    FOLLOWER_OF_CREATOR: 'Followers',
    SELF_ONLY: 'Only me'
  };
  const paidVisibility = new Set(['PUBLIC_TO_EVERYONE', 'MUTUAL_FOLLOW_FRIENDS']);
  const allowedToPost = new Set(['none', 'failed']);

  function mount({get, getSelection, getCaption, getConnection}) {
    const el = name => document.getElementById('tt-direct-' + name);
    const preview = el('preview'), title = el('title'), privacy = el('privacy');
    const message = text => { el('status').textContent = text; };
    const connection = () => getConnection ? getConnection() : {direct_post_granted: true};
    let key, generation = 0, creator = null, state = 'unknown', busy = false;
    let captionSeed, previewReady = false, timer, checking = false, lastReceipt = null;

    function selectionKey() {
      const clip = getSelection(), account = connection();
      return JSON.stringify([clip?.clipId, clip?.useSource, clip?.version,
        account?.account_id, account?.handle, Boolean(account), Boolean(account?.client_configured), Boolean(account?.direct_post_granted)]);
    }
    function current(turn) { return turn === generation && key === selectionKey(); }
    function path(action) {
      const clip = getSelection();
      return '/api/oauth/tiktok/direct/' + action + '/' + encodeURIComponent(clip.clipId);
    }
    function reset() {
      generation++;
      clearTimeout(timer);
      creator = null;
      lastReceipt = null;
      state = 'unknown';
      checking = busy = previewReady = false;
      el('form').hidden = true;
      el('check').hidden = true;
      el('post-link').hidden = true;
      el('post-link').removeAttribute('href');
      if (preview.hasAttribute('src')) { preview.pause(); preview.removeAttribute('src'); preview.load(); }
      el('auth-link').hidden = true;
      el('auth-link').removeAttribute('href');
      el('count').textContent = '';
      for (const input of el('fields').querySelectorAll('input[type=checkbox]')) input.checked = false;
      privacy.replaceChildren(new Option('Choose an audience', ''));
      title.value = '';
    }
    function render() {
      const enabled = Boolean(connection()?.direct_post_granted);
      const editable = Boolean(creator) && !busy && allowedToPost.has(state);
      el('review').textContent = creator ? 'Reload TikTok review' : 'Review TikTok post';
      el('review').disabled = busy || !getSelection() || !enabled;
      el('connect').hidden = enabled || !connection()?.client_configured;
      el('fields').disabled = !editable;
      for (const interaction of ['comment', 'duet', 'stitch']) {
        el(interaction).disabled = !creator || creator[interaction + '_disabled'] !== false;
      }
      const commercial = el('commercial').checked;
      const own = commercial && el('own-brand').checked;
      const paid = commercial && el('paid-brand').checked;
      el('brands').hidden = !commercial;
      el('paid-policy').hidden = !paid;
      const incompatible = Boolean(privacy.value) && !paidVisibility.has(privacy.value);
      el('paid-brand').disabled = incompatible;
      for (const option of privacy.options) {
        option.disabled = Boolean(option.value) && paid && !paidVisibility.has(option.value);
      }
      const disclosure = paid ? 'Your video will be labeled as “Paid partnership”.'
        : own ? 'Your video will be labeled as “Promotional content”.'
        : 'Select whose brand this video promotes.';
      el('disclosure').textContent = disclosure + (incompatible ? ' Third-party branded content requires Everyone or Friends.' : '');
      const units = title.value.length;
      el('count').textContent = units + ' / 2,200 UTF-16 units';
      el('count').className = units > 2200 ? 'warn' : 'muted';
      title.setAttribute('aria-invalid', String(units > 2200));
      const durationOK = creator && creator.duration_ms > 0 &&
        creator.duration_ms <= creator.max_video_post_duration_sec * 1000;
      el('publish').disabled = !editable || !previewReady || !durationOK || units > 2200 ||
        !privacy.value || !el('consent').checked || (commercial && !own && !paid) || (paid && incompatible);
      el('check').disabled = checking || busy;
    }
    function sync() {
      const next = selectionKey();
      if (key !== next) {
        key = next;
        reset();
        const account = connection();
        if (!getSelection()) message('Select a Film Roll clip to review.');
        else if (!account) message('Checking TikTok connection.');
        else if (!account.direct_post_granted) {
          message(account.client_configured ? 'Authorize publishing to review this post.'
            : 'Set up TikTok in Connections & System, then authorize publishing.');
        } else message('');
      } else if (creator && !busy && allowedToPost.has(state) && captionSeed !== getCaption()) {
        const revised = getCaption();
        if (title.value === captionSeed) { title.value = revised; el('consent').checked = false; }
        captionSeed = revised;
      }
      render();
    }
    function applyReceipt(receipt, turn) {
      lastReceipt = receipt;
      const labels = {
        none: '', failed: 'TikTok reported this attempt failed. You can review and try again.',
        pending: 'TikTok is processing your video. It may take a few minutes to appear.',
        complete: 'TikTok confirmed the video was posted.',
        needs_review: 'Check this attempt before starting another upload.'
      };
      state = Object.hasOwn(labels, receipt.state) ? receipt.state : 'unknown';
      el('check').hidden = state === 'none';
      const handle = (creator.creator_username || '').replace(/^@/, '');
      if (state === 'complete' && /^[a-z0-9_.]+$/i.test(handle) && /^\d+$/.test(receipt.post_id || '')) {
        el('post-link').href = 'https://www.tiktok.com/@' + encodeURIComponent(handle) + '/video/' + receipt.post_id;
        el('post-link').hidden = false;
      }
      const publicAvailability = state === 'complete' && receipt.privacy_level === 'PUBLIC_TO_EVERYONE' && !receipt.post_id
        ? 'TikTok accepted the Everyone post. Public availability is awaiting confirmation.' : '';
      message([publicAvailability || labels[state] || (state === 'none' ? '' : 'Post status unavailable.'), receipt.message].filter(Boolean).join(' '));
      clearTimeout(timer);
      if (state === 'pending') timer = setTimeout(() => { if (current(turn)) checkStatus(turn); }, 12000);
      render();
    }
    async function checkStatus(turn = generation) {
      if (checking || !creator || !current(turn)) return;
      checking = true;
      clearTimeout(timer);
      render();
      const query = new URLSearchParams({use_source: getSelection().useSource,
        sha256: creator.sha256, account_id: creator.account_id});
      try {
        const receipt = await get(path('status') + '?' + query);
        if (current(turn)) applyReceipt(receipt, turn);
      } catch (e) {
        if (current(turn)) {
          state = 'unknown';
          el('check').hidden = false;
          message('Could not confirm post status. ' + e.message);
        }
      } finally {
        if (current(turn)) { checking = false; render(); }
      }
    }
    el('review').onclick = async () => {
      sync();
      if (el('review').disabled) return;
      reset();
      const turn = generation, clip = getSelection();
      busy = true;
      render();
      message('Checking TikTok account and video…');
      try {
        const info = await get(path('creator') + '?use_source=' + clip.useSource);
        if (!current(turn)) return;
        const url = new URL(info.preview_url, location.origin);
        if (!info.preview_url || url.origin !== location.origin || !info.sha256 || !info.account_id ||
            !info.creator_nickname || !Array.isArray(info.privacy_level_options)) {
          throw Error('The video or TikTok account could not be verified.');
        }
        creator = info;
        for (const option of info.privacy_level_options) {
          if (Object.hasOwn(visibility, option)) privacy.add(new Option(visibility[option], option));
        }
        captionSeed = getCaption();
        title.value = captionSeed;
        el('account').textContent = 'Posting to ' + info.creator_nickname +
          (info.creator_username ? ' (@' + info.creator_username.replace(/^@/, '') + ')' : '');
        const tooLong = info.duration_ms > info.max_video_post_duration_sec * 1000;
        el('media').textContent = (clip.useSource ? 'Source video' : 'Exported video') + ' · ' +
          (info.duration_ms / 1000).toFixed(1) + ' seconds · Account limit ' + info.max_video_post_duration_sec +
          ' seconds.' + (tooLong ? ' Shorten the video before posting.' : '');
        el('form').hidden = false;
        preview.src = url.href;
        preview.load();
        await checkStatus(turn);
      } catch (e) {
        if (current(turn)) message('TikTok review unavailable. ' + e.message);
      } finally {
        if (current(turn)) { busy = false; render(); }
      }
    };
    preview.addEventListener('loadedmetadata', () => {
      previewReady = Boolean(creator && preview.currentSrc === preview.src && preview.readyState >= 1);
      render();
    });
    preview.addEventListener('error', () => {
      if (!creator || !preview.hasAttribute('src')) return;
      previewReady = false;
      el('media').textContent = 'Video preview unavailable. Reload the TikTok review to load the selected file.';
      render();
    });
    el('fields').addEventListener('input', e => {
      if (e.target !== el('consent')) el('consent').checked = false;
      if (!el('commercial').checked) el('own-brand').checked = el('paid-brand').checked = false;
      render();
    });
    function postBody() {
      return {use_source: getSelection().useSource, account_id: creator.account_id,
        sha256: creator.sha256, title: title.value, privacy_level: privacy.value,
        allow_comment: el('comment').checked, allow_duet: el('duet').checked, allow_stitch: el('stitch').checked,
        disclose_commercial: el('commercial').checked, brand_organic_toggle: el('own-brand').checked,
        brand_content_toggle: el('paid-brand').checked, is_aigc: el('ai').checked, consent: el('consent').checked};
    }
    function snapshot() {
      return JSON.stringify([generation, key, postBody()]);
    }
    function existingPublic() {
      return (state === 'pending' || state === 'complete') && lastReceipt?.privacy_level === 'PUBLIC_TO_EVERYONE';
    }
    function existingSnapshot() {
      return JSON.stringify([generation, key, creator.account_id, creator.sha256, 'existing-public']);
    }
    async function prepareForShip() {
      sync();
      el('review').scrollIntoView({block:'nearest'});
      if (!creator && !el('review').disabled) await el('review').onclick();
      if (!creator) return {ready:false, message:el('status').textContent || 'Open the TikTok post review.'};
      if (existingPublic()) return {ready:true, existing:lastReceipt, snapshot:existingSnapshot()};
      if (!allowedToPost.has(state)) {
        return {ready:false, message:(el('status').textContent || 'Check the existing TikTok post status.') +
          ' Deselect TikTok to ship only the remaining destinations.'};
      }
      if (!creator.privacy_level_options.includes('PUBLIC_TO_EVERYONE')) {
        return {ready:false, message:'TikTok has not made Everyone available for this account. Public shipping is unavailable.'};
      }
      if (privacy.value !== 'PUBLIC_TO_EVERYONE') {
        privacy.focus();
        return {ready:false, message:'Select Everyone in the TikTok review to ship publicly.'};
      }
      if (el('publish').disabled) {
        if (!el('consent').checked) el('consent').focus();
        return {ready:false, message:'Check the TikTok video, caption and disclosure settings, give consent, then click Ship Selected Platforms.'};
      }
      return {ready:true, snapshot:snapshot()};
    }
    async function submit(expected) {
      sync();
      if (el('publish').disabled || (expected !== undefined && expected !== snapshot())) {
        throw Error('Review the current TikTok video and settings before posting.');
      }
      const turn = generation, body = postBody();
      busy = true;
      el('consent').checked = false;
      render();
      message('Sending the reviewed video and caption to TikTok…');
      try {
        const receipt = await get(path('post'), {method:'POST', headers:{'Content-Type':'application/json'},
          body:JSON.stringify(body)});
        if (current(turn)) applyReceipt(receipt, turn);
        return receipt;
      } catch (e) {
        if (current(turn)) {
          state = 'unknown';
          el('check').hidden = false;
          message(e.message + ' Check post status before trying again.');
        }
        throw e;
      } finally {
        if (current(turn)) { busy = false; render(); }
      }
    }
    el('publish').onclick = () => submit().catch(() => {});
    async function publishReviewed(expected) {
      if (!expected) throw Error('Review the TikTok post before shipping.');
      sync();
      if (existingPublic() && expected === existingSnapshot()) return {...lastReceipt, reused:true};
      const receipt = await submit(expected);
      if (receipt.state !== 'pending' && receipt.state !== 'complete') {
        throw Error(receipt.message || 'TikTok did not confirm this post. Check its status before shipping again.');
      }
      return receipt;
    }
    el('check').onclick = () => checkStatus();
    el('connect').onclick = async () => {
      const button = el('connect'), link = el('auth-link');
      button.disabled = true;
      link.hidden = true;
      try {
        const auth = await get('/api/oauth/tiktok/start?mode=direct', {method:'POST'});
        if (auth.auth_url) { link.href = auth.auth_url; link.hidden = false; }
        message(auth.browser_opened ? 'Complete TikTok authorization in your browser.' : 'Open the TikTok authorization link to continue.');
      } catch (e) { message(e.message); }
      finally { button.disabled = false; }
    };
    sync();
    return {sync, prepareForShip, publishReviewed};
  }
  window.snapmaticTikTokDirect = {mount};
})();
