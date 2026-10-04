#include "model.h"
#include <cstdio>
#include <cstring>
#include <fstream>
#include <sstream>

namespace rlm {

bool load(const std::string& path, Mesh& out, std::string& err) {
	std::ifstream f(path, std::ios::binary);
	if (!f) {
		err = "missing " + path;
		return false;
	}
	char magic[4];
	uint32_t hdr[3];
	f.read(magic, 4);
	f.read(reinterpret_cast<char*>(hdr), sizeof(hdr));
	if (!f || std::memcmp(magic, "RLM1", 4) != 0) {
		err = "bad header in " + path;
		return false;
	}
	uint32_t nv = hdr[0], nt = hdr[1], ns = hdr[2];
	if (nv == 0 || nt == 0 || nv > 1000000 || nt > 1000000 || ns == 0 || ns > 8) {
		err = "bad sizes in " + path;
		return false;
	}
	out.pos.resize(size_t(nv) * 3);
	out.idx.resize(size_t(nt) * 3);
	out.colours.assign(ns, std::vector<uint8_t>(size_t(nt) * 4));
	f.read(reinterpret_cast<char*>(out.pos.data()), std::streamsize(out.pos.size() * sizeof(float)));
	f.read(reinterpret_cast<char*>(out.idx.data()), std::streamsize(out.idx.size() * sizeof(uint32_t)));
	for (auto& c : out.colours) f.read(reinterpret_cast<char*>(c.data()), std::streamsize(c.size()));
	if (!f) {
		err = "truncated " + path;
		return false;
	}
	for (uint32_t i : out.idx)
		if (i >= nv) {
			err = "bad index in " + path;
			return false;
		}
	return true;
}

static void loadLods(const std::string& base, std::vector<Mesh>& lods, std::string& log) {
	lods.clear();
	for (int i = 0; i < 8; i++) {
		Mesh m;
		std::string err;
		if (!load(base + "_lod" + std::to_string(i) + ".rlm", m, err)) break;
		lods.push_back(std::move(m));
	}
	char line[160];
	std::snprintf(line, sizeof(line), "%s %zu lods; ", base.substr(base.find_last_of("/\\") + 1).c_str(), lods.size());
	log += line;
}

bool Models::load(const std::string& folder, const std::string& preset, std::string& log) {
	log.clear();
	loadLods(folder + "/" + preset + "_body", body, log);
	loadLods(folder + "/wheel", wheel, log);
	loadLods(folder + "/ball", ball, log);
	hasAnchors = false;
	std::ifstream f(folder + "/" + preset + "_wheels.txt");
	std::string ln;
	int found = 0;
	while (std::getline(f, ln)) {
		std::istringstream s(ln);
		std::string name;
		float x, y, z;
		if (!(s >> name >> x >> y >> z)) continue;
		int i = name == "FL" ? 0 : name == "FR" ? 1 : name == "BL" ? 2 : name == "BR" ? 3 : -1;
		if (i < 0) continue;
		anchors[i][0] = x, anchors[i][1] = y, anchors[i][2] = z;
		found |= 1 << i;
	}
	hasAnchors = found == 15;
	return ok();
}

}
