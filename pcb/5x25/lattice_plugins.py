import pcbnew
import math

def rotate_vector(vec, angle_degrees):
    rads = math.radians(angle_degrees)
    x = vec.x * math.cos(rads) - vec.y * math.sin(rads)
    y = vec.x * math.sin(rads) + vec.y * math.cos(rads)
    return pcbnew.VECTOR2I(int(x), int(y))

def is_in_zone(pos, center, radius):
    """Check if a point is within the square keyswitch zone. Adds 0.01mm tolerance."""
    epsilon = pcbnew.FromMM(0.01) 
    return max(abs(pos.x - center.x), abs(pos.y - center.y)) <= (radius + epsilon)

def get_adj_map(tracks):
    adj = {}
    for t in tracks:
        pts = [t.GetPosition()] if t.GetClass() == "PCB_VIA" else [t.GetStart(), t.GetEnd()]
        for p in pts: adj.setdefault((p.x, p.y), []).append(t)
    return adj

def trace_connected_chain(start_item, zone_center, adj_map, scan_radius):
    """Follows a track/via chain and returns (chain_list, chain_ids, exit_points)."""
    chain, chain_ids, queue, exit_points = [], set(), [start_item], set()
    while queue:
        curr = queue.pop(0)
        c_id = id(curr)
        if c_id in chain_ids: continue
        chain.append(curr); chain_ids.add(c_id)
        
        pts = [curr.GetPosition()] if curr.GetClass() == "PCB_VIA" else [curr.GetStart(), curr.GetEnd()]
        
        for p in pts:
            if not is_in_zone(p, zone_center, scan_radius):
                exit_points.add((p.x, p.y))
            for neighbor in adj_map.get((p.x, p.y), []):
                if id(neighbor) not in chain_ids: queue.append(neighbor)
    return chain, chain_ids, exit_points

def capture_path_strict(src_ref, dst_ref, ref_rot_global, fp_map, adj_map, all_tracks):
    """
    Robust Path Capture: Finds ONLY the trace connecting src to dst.
    Returns: (template_items, set_of_track_ids)
    """
    s_fp, d_fp = fp_map.get(src_ref), fp_map.get(dst_ref)
    if not s_fp or not d_fp: return [], set()
    
    # Start BFS from ALL pads of the source
    start_pads = list(s_fp.Pads())
    queue = []
    visited = set()
    for p in start_pads:
        pos = p.GetPosition()
        queue.append((pos, []))
        visited.add((pos.x, pos.y))
        
        # Off-center check
        for t in all_tracks:
            # Fast proximity check (2mm)
            if abs(t.GetStart().x - pos.x) < pcbnew.FromMM(2) and abs(t.GetStart().y - pos.y) < pcbnew.FromMM(2):
                 t_start, t_end = t.GetStart(), t.GetEnd()
                 if p.HitTest(t_start):
                     queue.append((t_end, [t])); visited.add((t_end.x, t_end.y))
                 elif p.HitTest(t_end):
                     queue.append((t_start, [t])); visited.add((t_start.x, t_start.y))
    
    dst_pads = list(d_fp.Pads())
    found_path = None
    
    while queue:
        curr_pos, path = queue.pop(0)
        
        # Check Hit Dst
        hit_dst = False
        for dp in dst_pads:
            dp_pos = dp.GetPosition()
            if dp_pos.x == curr_pos.x and dp_pos.y == curr_pos.y: hit_dst = True; break
            if dp.HitTest(curr_pos): hit_dst = True; break
        if hit_dst: 
            found_path = path
            break
        
        # Expand
        c_key = (curr_pos.x, curr_pos.y)
        for tr in adj_map.get(c_key, []):
            if tr in path: continue
            
            if tr.GetClass() == "PCB_VIA": next_pos = tr.GetPosition()
            else:                          next_pos = tr.GetEnd() if (tr.GetStart().x == curr_pos.x and tr.GetStart().y == curr_pos.y) else tr.GetStart()
            
            n_key = (next_pos.x, next_pos.y)
            if n_key not in visited:
                visited.add(n_key)
                queue.append((next_pos, path + [tr]))
    
    if not found_path: return [], set()

    # Enrich with Vias
    path_points = set()
    for item in found_path:
        if item.GetClass() == "PCB_VIA": path_points.add((item.GetPosition().x, item.GetPosition().y))
        else:                            path_points.add((item.GetStart().x, item.GetStart().y)); path_points.add((item.GetEnd().x, item.GetEnd().y))
    
    final_path = list(found_path)
    added_ids = {id(i) for i in final_path}
    for pt in path_points:
        for n in adj_map.get(pt, []):
             if n.GetClass() == "PCB_VIA" and id(n) not in added_ids:
                 final_path.append(n); added_ids.add(id(n))
    found_path = final_path


    # Normalize Rotation
    path_origin = s_fp.GetPosition()
    s_rot = s_fp.GetOrientation().AsDegrees()
    rot_correction = -(s_rot - ref_rot_global)
    
    def rot_p(vec):
        rad = math.radians(rot_correction)
        c, s = math.cos(rad), math.sin(rad)
        return pcbnew.wxPoint(int(vec.x * c - vec.y * s), int(vec.x * s + vec.y * c))

    template_items = []
    track_ids = set()
    for segment in found_path:
        track_ids.add(id(segment))
        is_via = segment.GetClass() == "PCB_VIA"
        data = {'is_via': is_via, 'width': segment.GetWidth()}
        if is_via: 
            rel_p = segment.GetPosition() - path_origin
            data.update({'pos': rot_p(rel_p), 'drill': segment.GetDrill(), 'via_type': segment.GetViaType(), 'layers': [segment.TopLayer(), segment.BottomLayer()]})
        else:      
            rel_s = segment.GetStart() - path_origin; rel_e = segment.GetEnd() - path_origin
            data.update({'start': rot_p(rel_s), 'end': rot_p(rel_e), 'layer': segment.GetLayer()})
        template_items.append(data)
    return template_items, track_ids

def apply_template_item(board, data, t_pos, rot_delta, net):
    if data['is_via']:
        nv = pcbnew.PCB_VIA(board)
        nv.SetPosition(t_pos + rotate_vector(data['pos'], rot_delta))
        nv.SetWidth(data['width']); nv.SetDrill(data['drill']); nv.SetViaType(data['via_type'])
        nv.SetLayerPair(data['layers'][0], data['layers'][1])
        if net: nv.SetNet(net)
        board.Add(nv)
    else:
        nt = pcbnew.PCB_TRACK(board)
        nt.SetStart(t_pos + rotate_vector(data['start'], rot_delta)); nt.SetEnd(t_pos + rotate_vector(data['end'], rot_delta))
        nt.SetWidth(data['width']); nt.SetLayer(data['layer'])
        if net: nt.SetNet(net)
        board.Add(nt)

class LatticeAlignComponents(pcbnew.ActionPlugin):
    def defaults(self):
        self.name, self.category, self.description, self.show_toolbar_button = "Lattice: Align Components", "Lattice Board", "Align footprints and clone traces from template SW3", False

    def Run(self):
        board = pcbnew.GetBoard()
        REF_KEY_IDX, START_IDX, END_IDX, SCAN_RADIUS = 3, 1, 125, pcbnew.FromMM(8.5)

        print("Optimizing board data...")
        all_fps = list(board.GetFootprints()); fp_map = {f.GetReference(): f for f in all_fps}
        all_pads = list(board.GetPads()); all_tracks = list(board.GetTracks())
        adj = get_adj_map(all_tracks)
        
        net_map = {} 
        for pad in all_pads:
            for l in pad.GetLayerSet().Seq(): net_map[(pad.GetPosition().x, pad.GetPosition().y, l)] = pad.GetNet()
        for t in all_tracks:
            if t.GetClass() == "PCB_VIA":
                net_map[(t.GetPosition().x, t.GetPosition().y, t.TopLayer())] = t.GetNet(); net_map[(t.GetPosition().x, t.GetPosition().y, t.BottomLayer())] = t.GetNet()

        def capture_local_template(ref_idx):
            ref_sw = fp_map.get(f"SW{ref_idx}")
            if not ref_sw: return [], None, 0
            r_pos, r_rot = ref_sw.GetPosition(), ref_sw.GetOrientation().AsDegrees()
            ref_pads = {p.GetNumber(): p for p in ref_sw.Pads()}
            
            candidates = [t for t in all_tracks if is_in_zone(t.GetStart(), r_pos, SCAN_RADIUS) or is_in_zone(t.GetEnd(), r_pos, SCAN_RADIUS)]
            items, proc_ids = [], set()
            for item in candidates:
                if id(item) in proc_ids: continue
                chain, c_ids, exits = trace_connected_chain(item, r_pos, adj, SCAN_RADIUS)
                proc_ids.update(c_ids)
                if len(exits) == 0:
                    found_pad_num = None
                    for c_item in chain:
                        # Handle via pos or track ends
                        pts = [c_item.GetPosition()] if c_item.GetClass() == "PCB_VIA" else [c_item.GetStart(), c_item.GetEnd()]
                        for pt in pts:
                            for pn, pObj in ref_pads.items():
                                if pObj.HitTest(pt): found_pad_num = pn; break
                            if found_pad_num: break
                        if found_pad_num: break
                    
                    for segment in chain:
                        is_via = segment.GetClass() == "PCB_VIA"
                        data = {'is_via': is_via, 'width': segment.GetWidth(), 'pad_num': found_pad_num}
                        if is_via: data.update({'pos': segment.GetPosition() - r_pos, 'drill': segment.GetDrill(), 'via_type': segment.GetViaType(), 'layers': [segment.TopLayer(), segment.BottomLayer()]})
                        else:      data.update({'start': segment.GetStart() - r_pos, 'end': segment.GetEnd() - r_pos, 'layer': segment.GetLayer()})
                        items.append(data)
            return items, r_pos, r_rot

        tmpl_A, pos_A, rot_A = capture_local_template(3)
        tmpl_B, pos_B, rot_B = capture_local_template(14)

        for i in range(START_IDX, END_IDX + 1):
            if i == 3 or i == 14 or i == 2: continue 
            target_sw = fp_map.get(f"SW{i}")
            if not target_sw: continue
            
            offset = (i - 1) % 25
            use_A = offset < 12
            tmpl_items, ref_pos, ref_rot = (tmpl_A, pos_A, rot_A) if use_A else (tmpl_B, pos_B, rot_B)
            curr_ref_idx = 3 if use_A else 14
            t_pos, rot_delta = target_sw.GetPosition(), target_sw.GetOrientation().AsDegrees() - ref_rot
            
            for prefix in ["D", "C", "LED"]:
                ref, target = fp_map.get(f"{prefix}{curr_ref_idx}"), fp_map.get(f"{prefix}{i}")
                if ref and target:
                    target.SetPosition(t_pos + (ref.GetPosition() - ref_pos))
                    if target.IsFlipped() != ref.IsFlipped(): target.Flip(target.GetPosition(), True)
                    target.SetOrientation(ref.GetOrientation())

            to_remove = []; cleaned_ids = set()
            local_items = [t for t in all_tracks if is_in_zone(t.GetStart(), t_pos, SCAN_RADIUS) or is_in_zone(t.GetEnd(), t_pos, SCAN_RADIUS)]
            for item in local_items:
                if id(item) in cleaned_ids: continue
                chain, c_ids, exits = trace_connected_chain(item, t_pos, adj, SCAN_RADIUS)
                cleaned_ids.update(c_ids)
                if len(exits) == 0: to_remove.extend(chain)
            for item in to_remove:
                try: board.Remove(item)
                except: pass

            for data in tmpl_items:
                ref_pt = data.get('pos', data.get('start'))
                net = None
                if data.get('pad_num'):
                    pad = target_sw.FindPadByNumber(data['pad_num'])
                    if pad: net = pad.GetNet()
                if not net:
                    t_pt_map = t_pos + rotate_vector(ref_pt, rot_delta)
                    l = data.get('layers', [data.get('layer', 0)])[0]
                    net = net_map.get((t_pt_map.x, t_pt_map.y, l))
                apply_template_item(board, data, t_pos, rot_delta, net)

        pcbnew.Refresh()

class LatticeConnectLeds(pcbnew.ActionPlugin):
    def defaults(self):
        self.name, self.category, self.description, self.show_toolbar_button = "Lattice: Connect LEDs", "Lattice Board", "Connect LEDs in series chains of 3", False

    def Run(self):
        board = pcbnew.GetBoard()
        all_fps = list(board.GetFootprints()); fp_map = {f.GetReference(): f for f in all_fps}; all_tracks = list(board.GetTracks())
        adj = get_adj_map(all_tracks)

        f_ref = fp_map.get("LED2"); ref_rot = f_ref.GetOrientation().AsDegrees() if f_ref else 0
        
        preserved_ids = set()
        def cap(s, d): 
            t, ids = capture_path_strict(s, d, ref_rot, fp_map, adj, all_tracks)
            preserved_ids.update(ids)
            return t
        
        templates = {
            'H': cap("LED2", "LED3"), 'V': cap("LED3", "LED4"), 'D': cap("LED6", "LED7"),
            'H_b': cap("LED25", "LED24"), 'V_b': cap("LED20", "LED19"), 'D_b': cap("LED23", "LED22"),
            'A': cap("LED12", "LED25"), 'B': cap("LED38", "LED51")
        }

        # Cleanup (Preserving Templates)
        to_remove = []; cleaned_ids = set()
        for fp in all_fps:
            if not fp.GetReference().startswith("LED") or fp.GetReference() == "LED14": continue
            for p_num in ["2", "3"]:
                if fp.GetReference() == "LED26" and p_num == "3": continue
                pad = fp.FindPadByNumber(p_num)
                if pad:
                    radius = pcbnew.FromMM(1000) if p_num == "2" else pcbnew.FromMM(100)
                    
                    start_tracks = list(adj.get((pad.GetPosition().x, pad.GetPosition().y), []))
                    start_ids = {id(t) for t in start_tracks}
                    
                    # Check for off-center connections
                    for t in all_tracks:
                        if id(t) in start_ids: continue
                        # Fast bbox check
                        if abs(t.GetStart().x - pad.GetPosition().x) < pcbnew.FromMM(2) and abs(t.GetStart().y - pad.GetPosition().y) < pcbnew.FromMM(2):
                             if pad.HitTest(t.GetStart()) or pad.HitTest(t.GetEnd()):
                                 start_tracks.append(t); start_ids.add(id(t))
                    
                    for track in start_tracks:
                        if id(track) not in cleaned_ids and id(track) not in preserved_ids:
                            chain, c_ids, _ = trace_connected_chain(track, pad.GetPosition(), adj, radius)
                            safe_chain = []
                            for it in chain:
                                if id(it) not in preserved_ids: safe_chain.append(it)
                            to_remove.extend(safe_chain); cleaned_ids.update(c_ids)
        for item in to_remove:
            try: board.Remove(item)
            except: pass

        def apply(tmpl_name, src_idx):
            if src_idx == 14: return
            f_src = fp_map.get(f"LED{src_idx}")
            if f_src and templates.get(tmpl_name):
                t_pos, rot_delta = f_src.GetPosition(), f_src.GetOrientation().AsDegrees() - ref_rot
                for data in templates[tmpl_name]: 
                    apply_template_item(board, data, t_pos, rot_delta, f_src.FindPadByNumber("2").GetNet())

        for group in range(5):
            base = group * 25
            for t_base in [1, 4, 7, 10]:
                apply('H', base + t_base); apply('H', base + t_base + 1)
            apply('V', base + 3); apply('D', base + 6); apply('V', base + 9)
            apply('A', base + 12)
            for t_base in [25, 22, 19, 16]:
                apply('H_b', base + t_base); apply('H_b', base + t_base - 1)
            apply('D_b', base + 23); apply('V_b', base + 20); apply('D_b', base + 17); apply('V_b', base + 14)
            if group < 4: apply('B', base + 13)

        pcbnew.Refresh()

class LatticeConnectMatrix(pcbnew.ActionPlugin):
    def defaults(self):
        self.name, self.category, self.description, self.show_toolbar_button = "Lattice: Connect Matrix", "Lattice Board", "Connect Key Matrix (Diodes for ROWS, Switches for COLUMNS)", False

    def Run(self):
        board = pcbnew.GetBoard()
        all_fps = list(board.GetFootprints()); fp_map = {f.GetReference(): f for f in all_fps}; all_tracks = list(board.GetTracks())
        adj = get_adj_map(all_tracks)
        
        f_ref_d = fp_map.get("D2"); ref_rot_d = f_ref_d.GetOrientation().AsDegrees() if f_ref_d else 0
        f_ref_sw = fp_map.get("SW2"); ref_rot_sw = f_ref_sw.GetOrientation().AsDegrees() if f_ref_sw else 0
        
        preserved_ids = set()
        def cap(s, d, ref_rot):
            t, ids = capture_path_strict(s, d, ref_rot, fp_map, adj, all_tracks)
            preserved_ids.update(ids)
            return t

        row_tmpls = {
            'H': cap("D2", "D3", ref_rot_d), 
            'V_odd': cap("D3", "D4", ref_rot_d), 
            'D_odd': cap("D6", "D7", ref_rot_d), 
            'V_even': cap("D13", "D14", ref_rot_d), 
            'D_even': cap("D16", "D17", ref_rot_d), 
        }
        col_tmpls = {
            'V': cap("SW2", "SW15", ref_rot_sw),
            'D': cap("SW4", "SW17", ref_rot_sw),
            'Special': cap("SW38", "SW63", ref_rot_sw)
        }

        # Cleanup Matrix Traces (Excluded Preserved)
        cleaned_ids = set(); to_remove = []
        for fp in all_fps:
            ref = fp.GetReference()
            if ref.startswith("D") or ref.startswith("SW"):
                pad = fp.FindPadByNumber("1")
                if pad:
                     for track in adj.get((pad.GetPosition().x, pad.GetPosition().y), []):
                        if id(track) not in cleaned_ids and id(track) not in preserved_ids:
                             chain, c_ids, _ = trace_connected_chain(track, pad.GetPosition(), adj, pcbnew.FromMM(100))
                             safe_chain = []
                             for it in chain:
                                 if id(it) not in preserved_ids: safe_chain.append(it)
                             
                             to_remove.extend(safe_chain); cleaned_ids.update(c_ids)
        for item in to_remove:
            try: board.Remove(item)
            except: pass

        def apply(tmpl, src_ref, pin_num, global_ref_rot):
            if not tmpl: return
            f_src = fp_map.get(src_ref)
            if not f_src: return
            t_pos, rot_delta = f_src.GetPosition(), f_src.GetOrientation().AsDegrees() - global_ref_rot
            pad = f_src.FindPadByNumber(pin_num); net = pad.GetNet() if pad else None
            for data in tmpl: apply_template_item(board, data, t_pos, rot_delta, net)

        # ROWS
        for group in range(5):
             base = group * 25
             for tri_base in [1, 4, 7, 10]:
                 apply(row_tmpls['H'], f"D{base+tri_base}", "1", ref_rot_d)
                 apply(row_tmpls['H'], f"D{base+tri_base+1}", "1", ref_rot_d)
             apply(row_tmpls['V_odd'], f"D{base+3}", "1", ref_rot_d)
             apply(row_tmpls['D_odd'], f"D{base+6}", "1", ref_rot_d)
             apply(row_tmpls['V_odd'], f"D{base+9}", "1", ref_rot_d)
             apply(row_tmpls['V_even'], f"D{base+13}", "1", ref_rot_d)
             for tri_base in [14, 17, 20, 23]:
                 apply(row_tmpls['H'], f"D{base+tri_base}", "1", ref_rot_d)
                 apply(row_tmpls['H'], f"D{base+tri_base+1}", "1", ref_rot_d)
             apply(row_tmpls['D_even'], f"D{base+16}", "1", ref_rot_d)
             apply(row_tmpls['V_even'], f"D{base+19}", "1", ref_rot_d)
             apply(row_tmpls['D_even'], f"D{base+22}", "1", ref_rot_d)

        # COLS
        curr = 13
        while curr + 25 <= 125: apply(col_tmpls['Special'], f"SW{curr}", "1", ref_rot_sw); curr += 25
        
        for col_start in range(1, 13):
             # Group A (1-3, 7-9) Needs Left Turn (V template, 2->15) to hit targets like 1->14, 7->20
             # Group B (4-6, 10-12) Needs Right Turn (D template, 4->17) to hit targets like 6->19, 11->24
             starts_vert = (col_start in [1, 2, 3, 7, 8, 9])
             
             curr = col_start; step_idx = 0
             while True:
                  step = 13 if step_idx % 2 == 0 else 12
                  next_val = curr + step
                  if next_val > 125: break
                  
                  is_vert = (step_idx % 2 == 0) if starts_vert else (step_idx % 2 != 0)
                  tmpl = col_tmpls['V'] if is_vert else col_tmpls['D']
                  
                  apply(tmpl, f"SW{curr}", "1", ref_rot_sw)
                  curr = next_val; step_idx += 1
        
        pcbnew.Refresh()

print("Registering Lattice Plugins...")
LatticeAlignComponents().register()
LatticeConnectLeds().register()
LatticeConnectMatrix().register()
