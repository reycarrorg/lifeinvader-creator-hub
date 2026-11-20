"""Consumes saved flat-list JSON lines. No network, credentials, or media URL extraction."""
import json,re,time
from tiktok_window_policy import handle,clip

def classify(profile,raw):
    who=handle(profile)
    if len(raw)>262144:raise ValueError('listing_size')
    lines=raw.splitlines()
    if len(lines)>50:raise ValueError('listing_count')
    seen=set();result=[]
    def pairs(values):
        d=dict(values)
        if len(d)!=len(values):raise ValueError('duplicate_field')
        return d
    for line in lines:
        if not 0<len(line)<=32768:raise ValueError('listing_row_size')
        r=json.loads(line,object_pairs_hook=pairs)
        if type(r)!=dict:raise ValueError('listing_row')
        ident=r.get('id')
        owner=r.get('uploader_url') or r.get('channel_url')
        url=r.get('webpage_url') or r.get('url')
        item=clip(who,ident,owner,r.get('timestamp'),'unknown')
        if item['post_id'] is not None:
            if ident in seen:raise ValueError('duplicate_post')
            seen.add(ident)
        if item['reason']=='download_permission_unknown':
            prefix='https://www.tiktok.com/@'+who
            ex=r.get('extractor_key') or r.get('ie_key')
            if ex!='TikTok':item.update(reason='extractor_unknown')
            elif url==prefix+'/photo/'+ident or r.get('vcodec')=='none':item.update(state='EXCLUDED',reason='photo_or_no_video')
            elif url!=prefix+'/video/'+ident:item.update(reason='post_url_mismatch')
            elif r.get('availability') in ('private','needs_auth','subscriber_only','premium_only'):item.update(state='BLOCKED',reason='access_restricted')
            elif r.get('has_drm') is True:item.update(state='BLOCKED',reason='protected_media')
            elif r.get('availability')!='public' or r.get('has_drm') is not False or r.get('live_status') not in ('not_live','was_live'):item.update(reason='media_status_unknown')
            else:item.update(state='REVIEWABLE',reason='requires_separate_permission')
        result.append(item)
    return result
