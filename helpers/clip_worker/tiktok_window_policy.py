"""Read-only UI/discovery policy. No network calls, archive writes, or download authority."""
import re
from datetime import datetime,timezone
from email.utils import parsedate_to_datetime

SCHEMA='tiktok-window-review-v1'
HANDLES=frozenset(('demo_creator_one','demo_creator_two','demo_creator_three'))
START=int(datetime(2026,11,18,6,tzinfo=timezone.utc).timestamp())
END=int(datetime(2026,11,21,6,tzinfo=timezone.utc).timestamp())
PACE_SECONDS=2

def handle(value):
    if type(value)!=str or not value.isascii():raise ValueError('profile_scope')
    m=re.fullmatch(r'(?:@|https://(?:www\.)?tiktok\.com/@)?([A-Za-z0-9_.]{1,24})(/?)',value)
    if not m or (m[2] and not value.startswith('https://')) or m[1].lower() not in HANDLES:raise ValueError('profile_scope')
    return m[1].lower()

def clip(profile,post_id,author,created_utc,eligibility):
    target=handle(profile)
    valid_id=type(post_id)==str and re.fullmatch(r'[0-9]{15,22}',post_id) is not None
    rec=dict(schema=SCHEMA,profile=target,post_id=post_id if valid_id else None,state='RECHECK',reason=None,dispatch=False)
    if not valid_id:reason='missing_or_invalid_post_id'
    elif author is None:reason='author_unknown'
    else:
        try:owner=handle(author)
        except ValueError:owner=None
        if owner!=target:reason='author_mismatch';rec['state']='EXCLUDED'
        elif type(created_utc)!=int or not 0<created_utc<=4102444800:reason='utc_timestamp_unknown'
        elif not START<=created_utc<END:reason='outside_cst_window';rec['state']='EXCLUDED'
        elif eligibility=='creator_disabled':reason='creator_download_disabled';rec['state']='BLOCKED'
        elif eligibility=='auth_required':reason='authentication_required';rec['state']='BLOCKED'
        elif eligibility=='drm':reason='protected_media';rec['state']='BLOCKED'
        elif eligibility=='native_save_observed':reason='native_save_observed';rec['state']='REVIEWABLE'
        else:reason='download_permission_unknown'
    rec['reason']=reason
    return rec

def retry_after(value,now):
    if type(value)!=str or len(value)>128:return None
    if re.fullmatch(r'[0-9]{1,10}',value):return now+int(value)
    try:
        dt=parsedate_to_datetime(value)
        if dt.tzinfo is None:return None
        return max(now,int(dt.timestamp()))
    except (TypeError,ValueError,OverflowError):return None

def next_read(status,now,*,scope='metadata_get',issue=None,header=None):
    if type(now)!=int or now<=0:raise ValueError('clock_unknown')
    # Only schedule a caller-controlled new read. No automatic retries.
    result=dict(reason='unknown_outcome',not_before_utc=None,retryable=False)
    if scope!='metadata_get':return result
    if issue in ('creator_disabled','auth_required','drm'):return dict(result,reason=issue)
    if status in (401,403):return dict(result,reason='access_restricted')
    if status==429:
        delay=retry_after(header,now)
        if delay is None:return dict(result,reason='rate_limit_wait_unknown')
        return dict(reason='rate_limited',retryable=True,not_before_utc=max(now+60,delay))
    if status in (502,503,504) or (status is None and issue in ('connect_timeout','connection_refused')):
        delay=retry_after(header,now) if header is not None else now
        if delay is None:return dict(result,reason='retry_wait_unknown')
        return dict(reason='transient_read_failure',retryable=True,not_before_utc=max(now+30,delay))
    if status==200:return dict(result,reason='read_complete',not_before_utc=now+PACE_SECONDS)
    return result
